pub(crate) mod cheatsheets;
pub(crate) mod wifi;

use cheatsheets::{Cheatsheet, Cheatsheets};
use core::convert::Infallible;
use core::fmt::Display;
use core::time::Duration;
use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_9X15};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{self, Gray8};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, StyledDrawable};
use embedded_graphics::text::Text;
use inkview::bindings::Inkview;
use inkview_eg::InkviewDisplay;
use pb_cheatsheet_com::grpc::PbCheatsheetServerImpl;
use pb_cheatsheet_com::{
    CheatsheetImage, CheatsheetsInfo, FocusedWindowInfo, ScreenInfo, PB_GRPC_PORT,
};
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing_subscriber::fmt::writer::MakeWriterExt;

const PB_GRPC_ADDR: &str = const_format::formatcp!("0.0.0.0:{PB_GRPC_PORT}");
const CLIENT_DATA_DIR: &str = "/mnt/ext1/applications/pb-cheatsheet-data";
const CHEATSHEETS_SUBFOLDER: &str = "cheatsheets";
const LOG_FILE_NAME: &str = "pb-cheatsheet.log";

#[derive(Debug)]
enum Msg {
    InkviewEvent(inkview::Event),
    FocusedWindow(FocusedWindowInfo),
    GetScreenInfo(oneshot::Sender<anyhow::Result<ScreenInfo>>),
    GetCheatsheetsInfo(oneshot::Sender<anyhow::Result<CheatsheetsInfo>>),
    UploadCheatsheet {
        image: CheatsheetImage,
        name: String,
        tags: HashSet<String>,
    },
    RemoveCheatsheet {
        name: String,
    },
    UploadScreenshot {
        screenshot: CheatsheetImage,
        name: String,
    },
    ClearScreenshot,
    AddCheatsheetTags {
        name: String,
        tags: HashSet<String>,
    },
    RemoveCheatsheetTags {
        name: String,
        tags: HashSet<String>,
    },
    AddWmClassTags {
        wm_class: String,
        tags: HashSet<String>,
    },
    RemoveWmClassTags {
        wm_class: String,
        tags: HashSet<String>,
    },
}

#[derive(Debug)]
struct GrpcServer {
    tx: mpsc::UnboundedSender<Msg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum UiMode {
    /// Switch manually through all cheatsheets
    Manual,
    /// Automatic cheatsheet page switching dependending on matched tags based on the current reported wm class
    AutomaticWmClass,
    /// Screenshot
    Screenshot,
}

impl Default for UiMode {
    fn default() -> Self {
        Self::AutomaticWmClass
    }
}

impl Display for UiMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiMode::Manual => write!(f, "M"),
            UiMode::AutomaticWmClass => write!(f, "A-WMC"),
            UiMode::Screenshot => write!(f, "SCR"),
        }
    }
}

impl UiMode {
    const CYCLE_TIME: Duration = Duration::from_millis(1000);

    fn prev(&mut self) {
        *self = match self {
            Self::Manual => Self::Manual,
            Self::AutomaticWmClass => Self::Manual,
            Self::Screenshot => Self::AutomaticWmClass,
        };
    }
    fn next(&mut self) {
        *self = match self {
            Self::Manual => Self::AutomaticWmClass,
            Self::AutomaticWmClass => Self::Screenshot,
            Self::Screenshot => Self::Screenshot,
        };
    }
}

#[derive(Debug, Default)]
struct UiState {
    pub mode: UiMode,
    pub focused_window_info: FocusedWindowInfo,
    pub screen_info: ScreenInfo,
    pub cheatsheets: Cheatsheets,
    /// Current pages for wm class
    pub current_page: HashMap<String, usize>,
    pub manual_mode_current_page: usize,
    pub screenshot: Option<(Cheatsheet, String)>,
    pub show_stats: bool,
    pub button_prev_pressed_time: Option<Instant>,
    pub button_next_pressed_time: Option<Instant>,
}

impl UiState {
    async fn with_loaded_cheatsheets() -> anyhow::Result<Self> {
        let cheatsheets =
            Cheatsheets::load_from_path(PathBuf::from(CLIENT_DATA_DIR).join(CHEATSHEETS_SUBFOLDER))
                .await?;
        Ok(Self {
            mode: UiMode::default(),
            focused_window_info: FocusedWindowInfo::default(),
            screen_info: ScreenInfo::default(),
            cheatsheets,
            current_page: HashMap::default(),
            screenshot: None,
            show_stats: false,
            button_prev_pressed_time: None,
            button_next_pressed_time: None,
            manual_mode_current_page: 0,
        })
    }

    pub fn update(&mut self, _iv: &'static Inkview, display: &InkviewDisplay) {
        self.screen_info = ScreenInfo {
            width: display.iv_screen_ref().width() as u32,
            height: display.iv_screen_ref().height() as u32,
            orientation: screen_orientation_iv_to_com(display.iv_screen_ref().orientation()),
        };
    }

    /// Switch to previous page. Dependent on the UI mode.
    ///
    /// Returns: boolean whether switch page happened
    pub fn prev_page(&mut self) -> bool {
        match self.mode {
            UiMode::Manual => {
                let current_page = self.manual_mode_current_page;
                let prev_page = self.manual_mode_current_page.saturating_sub(1);
                if prev_page != current_page {
                    self.manual_mode_current_page = prev_page;
                    true
                } else {
                    false
                }
            }
            UiMode::AutomaticWmClass => {
                let current_page = if let Some(p) = self
                    .current_page
                    .get_mut(&self.focused_window_info.wm_class)
                {
                    p
                } else {
                    self.current_page
                        .insert(self.focused_window_info.wm_class.clone(), 0);
                    self.current_page
                        .get_mut(&self.focused_window_info.wm_class)
                        .unwrap()
                };
                let prev_page = current_page.saturating_sub(1);
                if prev_page != *current_page {
                    *current_page = prev_page;
                    true
                } else {
                    false
                }
            }
            UiMode::Screenshot => false,
        }
    }

    /// Switch to next page. Dependent on the UI mode.
    ///
    /// Returns: boolean whether switch page happened
    pub fn next_page(&mut self) -> bool {
        match self.mode {
            UiMode::Manual => {
                let pages = self.cheatsheets.sheets_iter().count();
                let current_page = self.manual_mode_current_page;
                let next_page = self
                    .manual_mode_current_page
                    .saturating_add(1)
                    .min(pages.saturating_sub(1));
                if next_page != current_page {
                    self.manual_mode_current_page = next_page;
                    true
                } else {
                    false
                }
            }
            UiMode::AutomaticWmClass => {
                let n_sheets = self
                    .cheatsheets
                    .wm_class_n_sheets(&self.focused_window_info.wm_class);
                let current_page = if let Some(p) = self
                    .current_page
                    .get_mut(&self.focused_window_info.wm_class)
                {
                    p
                } else {
                    self.current_page
                        .insert(self.focused_window_info.wm_class.clone(), 0);
                    self.current_page
                        .get_mut(&self.focused_window_info.wm_class)
                        .unwrap()
                };
                let next_page = current_page
                    .saturating_add(1)
                    .min(n_sheets.saturating_sub(1));
                if next_page != *current_page {
                    *current_page = next_page;
                    true
                } else {
                    false
                }
            }
            UiMode::Screenshot => false,
        }
    }

    pub fn draw_to_display(
        &mut self,
        display: &mut impl DrawTarget<Color = pixelcolor::Gray8, Error = Infallible>,
    ) -> anyhow::Result<()> {
        const TEXT_STYLE_NORMAL: MonoTextStyle<Gray8> =
            MonoTextStyle::new(&FONT_9X15, pixelcolor::Gray8::new(0x00));
        const TEXT_STYLE_HUGE: MonoTextStyle<Gray8> =
            MonoTextStyle::new(&FONT_10X20, pixelcolor::Gray8::new(0x00));
        const STROKE_THIN_BLACK: PrimitiveStyle<Gray8> =
            PrimitiveStyle::with_stroke(Gray8::new(0x00), 1);
        const FILL_WHITE: PrimitiveStyle<Gray8> = PrimitiveStyle::with_fill(Gray8::new(0xff));
        let display_bounding_box = display.bounding_box();
        let display_center = display_bounding_box.center();

        fn draw_ui_info(
            display: &mut impl DrawTarget<Color = pixelcolor::Gray8, Error = Infallible>,
            mode: UiMode,
            page: usize,
        ) -> anyhow::Result<()> {
            let display_bounding_box = display.bounding_box();
            let mode_string = mode.to_string();
            let ui_info_string = format!("{mode_string}:{page}");
            let ui_info_text = Text::new(
                &ui_info_string,
                Point::new(display_bounding_box.bottom_right().unwrap().x - 80, 30),
                TEXT_STYLE_HUGE,
            );
            let ui_info_text_bounding_box = ui_info_text.bounding_box();
            ui_info_text_bounding_box
                .into_styled(FILL_WHITE)
                .draw(display)?;
            ui_info_text_bounding_box
                .into_styled(STROKE_THIN_BLACK)
                .draw(display)?;
            ui_info_text.draw(display)?;
            Ok(())
        }

        display.clear(Gray8::new(0xff))?;

        match self.mode {
            UiMode::Manual => {
                let current_page = self.manual_mode_current_page;
                if let Some((name, (_metadata, sheet))) =
                    self.cheatsheets.sheets_iter().nth(current_page)
                {
                    sheet.draw(display)?;

                    let cheatsheet_name_text = Text::new(name, Point::new(10, 30), TEXT_STYLE_HUGE);
                    let cheatsheet_name_text_boundings_box = cheatsheet_name_text.bounding_box();
                    cheatsheet_name_text_boundings_box
                        .into_styled(FILL_WHITE)
                        .draw(display)?;
                    cheatsheet_name_text_boundings_box
                        .into_styled(STROKE_THIN_BLACK)
                        .draw(display)?;
                    cheatsheet_name_text.draw(display)?;
                }
                draw_ui_info(display, self.mode, current_page)?;
            }
            UiMode::AutomaticWmClass => {
                let current_page =
                    if let Some(p) = self.current_page.get(&self.focused_window_info.wm_class) {
                        *p
                    } else {
                        let page = 0;
                        self.current_page
                            .insert(self.focused_window_info.wm_class.clone(), page);
                        page
                    };
                if let Some((_metadata, sheet)) = self
                    .cheatsheets
                    .sheets_for_wm_class(&self.focused_window_info.wm_class)
                    .into_iter()
                    .nth(current_page)
                {
                    sheet.draw(display)?;
                } else {
                    let placeholder_text = Text::with_alignment(
                        "NO CHEATSHEET FOUND",
                        display_center,
                        TEXT_STYLE_HUGE,
                        embedded_graphics::text::Alignment::Center,
                    );
                    let placeholder_text_bounding_box = Rectangle::new(
                        placeholder_text.bounding_box().top_left - Point::new(10, 10),
                        placeholder_text.bounding_box().size + Size::new(20, 20),
                    );
                    placeholder_text_bounding_box
                        .into_styled(FILL_WHITE)
                        .draw(display)?;
                    placeholder_text_bounding_box
                        .into_styled(STROKE_THIN_BLACK)
                        .draw(display)?;
                    placeholder_text.draw(display)?;
                }
                draw_ui_info(display, self.mode, current_page)?;
            }
            UiMode::Screenshot => {
                if let Some((screenshot, _name)) = self.screenshot.as_ref() {
                    screenshot.draw(display)?;
                    // TODO: draw name
                } else {
                    let placeholder_text = Text::with_alignment(
                        "NO SCREENSHOT FOUND",
                        display_center,
                        TEXT_STYLE_HUGE,
                        embedded_graphics::text::Alignment::Center,
                    );
                    let placeholder_text_bounding_box = Rectangle::new(
                        placeholder_text.bounding_box().top_left - Point::new(10, 10),
                        placeholder_text.bounding_box().size + Size::new(20, 20),
                    );
                    placeholder_text_bounding_box
                        .into_styled(FILL_WHITE)
                        .draw(display)?;
                    placeholder_text_bounding_box
                        .into_styled(STROKE_THIN_BLACK)
                        .draw(display)?;
                    placeholder_text.draw(display)?;
                }

                draw_ui_info(display, self.mode, 0)?;
            }
        }

        if self.show_stats {
            let stats = format!(
                "
### Screen Info ###
    width:              {}
    height:             {}
    orientation:        {}

### Focused Window Info ###
    wm_class:           {}
    wm_class_instance:  {}
    pid:                {}
    focus:              {}
",
                self.screen_info.width,
                self.screen_info.height,
                self.screen_info.orientation,
                self.focused_window_info.wm_class,
                self.focused_window_info.wm_class_instance,
                self.focused_window_info.pid,
                self.focused_window_info.focus,
            );
            let stats_text = Text::new(&stats, Point::new(10, 40), TEXT_STYLE_NORMAL);
            let stats_text_bounding_box = stats_text.bounding_box();
            stats_text_bounding_box.draw_styled(&FILL_WHITE, display)?;
            stats_text.draw(display)?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl PbCheatsheetServerImpl for GrpcServer {
    async fn handle_focused_window(&self, info: FocusedWindowInfo) {
        if self.tx.send(Msg::FocusedWindow(info)).is_err() {
            tracing::error!(
                "Sending received GRPC focused window info from handler failed, receiving half closed"
            );
        }
    }

    async fn handle_get_screen_info(&self) -> anyhow::Result<ScreenInfo> {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Msg::GetScreenInfo(tx)).is_err() {
            return Err(anyhow::anyhow!(
                "Sending received GRPC get screen info sender from handler failed, receiving half closed"
            ));
        }
        let Ok(res) = rx.await else {
            return Err(anyhow::anyhow!(
                "Receiving request screen info failed, sender half dropped"
            ));
        };
        res
    }

    async fn handle_get_cheatsheets_info(&self) -> anyhow::Result<CheatsheetsInfo> {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Msg::GetCheatsheetsInfo(tx)).is_err() {
            return Err(anyhow::anyhow!(
                "Sending received GRPC get cheatsheets info sender from handler failed, receiving half closed"
            ));
        }
        let Ok(res) = rx.await else {
            return Err(anyhow::anyhow!(
                "Receiving request cheatsheets info failed, sender half dropped"
            ));
        };
        res
    }

    async fn handle_upload_cheatsheet(
        &self,
        image: CheatsheetImage,
        name: String,
        tags: HashSet<String>,
    ) {
        if self
            .tx
            .send(Msg::UploadCheatsheet { image, name, tags })
            .is_err()
        {
            tracing::error!(
                "Sending received GRPC cheatsheet image from handler failed, receiving half closed"
            );
        }
    }

    async fn handle_remove_cheatsheet(&self, name: String) {
        if self.tx.send(Msg::RemoveCheatsheet { name }).is_err() {
            tracing::error!("Sending remove cheatsheet message failed, receiving half closed");
        }
    }

    async fn handle_upload_screenshot(&self, screenshot: CheatsheetImage, name: String) {
        if self
            .tx
            .send(Msg::UploadScreenshot { screenshot, name })
            .is_err()
        {
            tracing::error!(
                "Sending received GRPC cheatsheet image from handler failed, receiving half closed"
            );
        }
    }

    async fn handle_clear_screenshot(&self) {
        if self.tx.send(Msg::ClearScreenshot).is_err() {
            tracing::error!(
                "Sending received GRPC screenshot from handler failed, receiving half closed"
            );
        }
    }

    async fn handle_add_cheatsheet_tags(&self, name: String, tags: HashSet<String>) {
        if self.tx.send(Msg::AddCheatsheetTags { name, tags }).is_err() {
            tracing::error!("Sending add cheatsheet tags message failed, receiving half closed");
        }
    }

    async fn handle_remove_cheatsheet_tags(&self, name: String, tags: HashSet<String>) {
        if self
            .tx
            .send(Msg::RemoveCheatsheetTags { name, tags })
            .is_err()
        {
            tracing::error!("Sending remove cheatsheet tags message failed, receiving half closed");
        }
    }

    async fn handle_add_wm_class_tags(&self, wm_class: String, tags: HashSet<String>) {
        if self
            .tx
            .send(Msg::AddWmClassTags { wm_class, tags })
            .is_err()
        {
            tracing::error!("Sending add wm class tags message failed, receiving half closed");
        }
    }

    async fn handle_remove_wm_class_tags(&self, wm_class: String, tags: HashSet<String>) {
        if self
            .tx
            .send(Msg::RemoveWmClassTags { wm_class, tags })
            .is_err()
        {
            tracing::error!("Sending add wm class tags message failed, receiving half closed");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_data_dir().await?;
    let logfile_guard = setup_logging()?;
    let iv: &'static inkview::bindings::Inkview = Box::leak(Box::new(inkview::load())) as &_;
    // The cancel token is used to indicated when the app should be quit
    let quit_token = tokio_util::sync::CancellationToken::new();
    // The exit cleanup token is used to block the main loop while cleanup tasks are running.
    let exit_cleanup_token = tokio_util::sync::CancellationToken::new();
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Msg>();
    let (file_save_tx, mut file_save_rx) = mpsc::unbounded_channel::<(PathBuf, Vec<u8>)>();

    // File save task
    let exit_cleanup_token_c = exit_cleanup_token.clone();
    let file_save_task = tokio::spawn(async move {
        while let Some((file_path, data)) = file_save_rx.recv().await {
            tracing::debug!("Saving file with path '{}'", file_path.display());
            if let Err(e) = async {
                fs::create_dir_all(file_path.parent().ok_or_else(|| {
                    anyhow::anyhow!("File '{}' does not have parent", file_path.display())
                })?)
                .await?;
                let mut file = fs::File::create(&file_path).await?;
                file.write_all(&data).await?;
                file.flush().await?;
                Result::<(), anyhow::Error>::Ok(())
            }
            .await
            {
                tracing::error!("Saving file '{}' failed, Err: {e:?}'", file_path.display());
            }
        }
        tracing::debug!("File save task finished, sender closed");
    });

    // GRPC server task
    let quit_token_c = quit_token.clone();
    let msg_tx_c = msg_tx.clone();
    tokio::spawn(async move {
        let grpc_server = GrpcServer { tx: msg_tx_c };
        println!("Starting GRPC server with listening address: '{PB_GRPC_ADDR}'");

        tokio::select! {
            _ = pb_cheatsheet_com::grpc::start_server(grpc_server, PB_GRPC_ADDR.parse().unwrap()) => {}
            _ = quit_token_c.cancelled() => {}
        }
    });

    let mut ui_state = UiState::with_loaded_cheatsheets()
        .await
        .inspect_err(|e| tracing::error!("Display state image loading failed, Err: {e:?}"))
        .unwrap_or_default();

    // Msg handle task
    tokio::task::spawn_blocking(move || {
        let mut display: OnceCell<InkviewDisplay> = OnceCell::new();

        loop {
            let mut repaint = false;

            let msg = msg_rx.blocking_recv();
            tracing::debug!("Handling received message:\n{msg:?}");
            let Some(msg) = msg else {
                continue;
            };

            match msg {
                Msg::InkviewEvent(event) => {
                    #[allow(clippy::collapsible_match, clippy::single_match)]
                    match event {
                        inkview::Event::Init => {
                            // UNIX signals quit task.
                            // *Must* be initialized after inkview's main,
                            // otherwise it will be overwritten
                            let quit_token_c = quit_token.clone();
                            tokio::spawn(async move {
                                let mut stream_sigquit = tokio::signal::unix::signal(
                                    tokio::signal::unix::SignalKind::quit(),
                                )
                                .expect("Create SIGQUIT signal stream");
                                let mut stream_sigterm = tokio::signal::unix::signal(
                                    tokio::signal::unix::SignalKind::terminate(),
                                )
                                .expect("Create SIGTERM signal stream");
                                let mut stream_sigint = tokio::signal::unix::signal(
                                    tokio::signal::unix::SignalKind::interrupt(),
                                )
                                .expect("Create SIGINT signal stream");
                                let mut stream_sighup = tokio::signal::unix::signal(
                                    tokio::signal::unix::SignalKind::hangup(),
                                )
                                .expect("Create SIGHUP signal stream");
                                tokio::select! {
                                    _ = stream_sigquit.recv() => {},
                                    _ = stream_sigterm.recv() => {},
                                    _ = stream_sigint.recv() => {},
                                    _ = stream_sighup.recv() => {},
                                }
                                quit_token_c.cancel();
                            });

                            // Initialize display
                            if display.set(InkviewDisplay::new(iv)).is_err() {
                                tracing::error!("Inkview display was already initialized when inkview init event was received");
                            }

                            // Wifi keep-alive task
                            let quit_token_c = quit_token.clone();
                            tokio::spawn(async move {
                                let mut interval = tokio::time::interval(Duration::from_secs(30));

                                loop {
                                    tokio::select! {
                                        _ = interval.tick() => {
                                            if let Err(e) = wifi::wifi_keepalive(iv) {
                                                tracing::error!("Wifi keep-alive failed, Err: {e:?}");
                                            }
                                        }
                                        _ = quit_token_c.cancelled() => break,
                                    }
                                }
                            });

                            repaint = true;
                        }
                        inkview::Event::Show | inkview::Event::Repaint => repaint = true,
                        inkview::Event::KeyDown { key } => match key {
                            inkview::event::Key::Prev => {
                                ui_state.button_prev_pressed_time = Some(Instant::now());
                            }
                            inkview::event::Key::Next => {
                                ui_state.button_next_pressed_time = Some(Instant::now());
                            }
                            inkview::event::Key::Menu => {
                                ui_state.show_stats = !ui_state.show_stats;
                                repaint = true;
                            }
                            _ => {}
                        },
                        inkview::Event::KeyUp { key } => match key {
                            inkview::event::Key::Prev => {
                                let now = Instant::now();
                                if let Some(earlier) = ui_state.button_prev_pressed_time.take() {
                                    #[allow(clippy::collapsible_else_if)]
                                    if now.duration_since(earlier) >= UiMode::CYCLE_TIME {
                                        ui_state.mode.prev();
                                        repaint = true;
                                    } else {
                                        if ui_state.prev_page() {
                                            repaint = true;
                                        }
                                    }
                                }
                            }
                            inkview::event::Key::Next => {
                                let now = Instant::now();
                                if let Some(earlier) = ui_state.button_next_pressed_time.take() {
                                    #[allow(clippy::collapsible_else_if)]
                                    if now.duration_since(earlier) >= UiMode::CYCLE_TIME {
                                        ui_state.mode.next();
                                        repaint = true;
                                    } else {
                                        if ui_state.next_page() {
                                            repaint = true;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },
                        inkview::Event::Exit => quit_token.cancel(),
                        _ => {}
                    }
                }
                Msg::FocusedWindow(info) => {
                    if info.wm_class != ui_state.focused_window_info.wm_class
                        && ui_state.mode == UiMode::AutomaticWmClass
                    {
                        repaint = true;
                    }
                    ui_state.focused_window_info = info;
                }
                Msg::GetScreenInfo(tx) => {
                    if let Some(display) = display.get() {
                        let size = display.size();
                        let orientation = display.iv_screen_ref().orientation();
                        if tx
                            .send(Ok(ScreenInfo {
                                width: size.width,
                                height: size.height,
                                orientation: screen_orientation_iv_to_com(orientation),
                            }))
                            .is_err()
                        {
                            tracing::error!("Sending screen info answer over channel failed, receiver half dropped");
                        }
                    } else {
                        tracing::warn!(
                            "Display not initialized yet when trying to retrieve its dimensions"
                        );
                    }
                }
                Msg::GetCheatsheetsInfo(tx) => {
                    let info = ui_state.cheatsheets.get_info();
                    if tx.send(Ok(info)).is_err() {
                        tracing::error!(
                            "Sending cheatsheets info over channel failed, receiver half dropped"
                        );
                    }
                }
                Msg::UploadCheatsheet { image, name, tags } => {
                    ui_state
                        .cheatsheets
                        .insert_sheet(Cheatsheet { image }, name, tags);
                    let n_sheets = ui_state
                        .cheatsheets
                        .wm_class_n_sheets(&ui_state.focused_window_info.wm_class);
                    if let Some(page) = ui_state
                        .current_page
                        .get_mut(&ui_state.focused_window_info.wm_class)
                    {
                        *page = page.saturating_add(1).min(n_sheets.saturating_sub(1));
                    } else {
                        ui_state
                            .current_page
                            .insert(ui_state.focused_window_info.wm_class.clone(), 0);
                    };

                    if let Err(e) = ui_state.cheatsheets.dispatch_save_to_path(
                        PathBuf::from(CLIENT_DATA_DIR).join(CHEATSHEETS_SUBFOLDER),
                        file_save_tx.clone(),
                    ) {
                        tracing::error!("Failed to dispatch saving cheatsheets, Err: {e:?}");
                    };
                    repaint = true;
                }
                Msg::RemoveCheatsheet { name } => {
                    ui_state.cheatsheets.remove_sheet(&name);
                }
                Msg::UploadScreenshot { screenshot, name } => {
                    ui_state.screenshot = Some((Cheatsheet { image: screenshot }, name));
                    ui_state.mode = UiMode::Screenshot;
                    repaint = true;
                }
                Msg::ClearScreenshot => {
                    ui_state.screenshot.take();
                    repaint = true;
                }
                Msg::AddCheatsheetTags { name, tags } => {
                    for tag in tags {
                        if let Err(e) = ui_state.cheatsheets.add_sheet_tag(&name, tag) {
                            tracing::error!("Failed to add tag to cheatsheet '{name}', Err: {e:?}");
                        }
                    }
                }
                Msg::RemoveCheatsheetTags { name, tags } => {
                    for tag in tags {
                        if let Err(e) = ui_state.cheatsheets.remove_sheet_tag(&name, &tag) {
                            tracing::error!(
                                "Failed to remove tag '{tag}' from cheatsheet '{name}', Err: {e:?}"
                            );
                        }
                    }
                }
                Msg::AddWmClassTags { wm_class, tags } => {
                    for tag in tags {
                        ui_state.cheatsheets.add_wm_class_tag(&wm_class, tag);
                    }
                }
                Msg::RemoveWmClassTags { wm_class, tags } => {
                    for tag in tags {
                        if let Err(e) = ui_state.cheatsheets.remove_wm_class_tag(&wm_class, &tag) {
                            tracing::error!(
                                "Failed to remove tag '{tag}' from wm class '{wm_class}', Err: {e:?}"
                            );
                        }
                    }
                }
            }

            if repaint {
                let Some(display) = display.get_mut() else {
                    tracing::warn!("Display not initialized yet when trying to repaint.");
                    return;
                };
                ui_state.update(iv, display);

                if let Err(e) = ui_state.draw_to_display(display) {
                    tracing::error!("Drawing display state failed, Err: {e:?}");
                }
                display.flush();
            }

            if quit_token.is_cancelled() {
                tracing::debug!("Quitting! Saving cheatsheets and metadata to files");

                if let Err(e) = ui_state.cheatsheets.dispatch_save_to_path(
                    PathBuf::from(CLIENT_DATA_DIR).join(CHEATSHEETS_SUBFOLDER),
                    file_save_tx,
                ) {
                    tracing::error!("Failed to dispatch saving images on exit, Err: {e:?}");
                };

                break;
            }
        }

        println!("Exiting..");
        tokio::runtime::Handle::current().block_on(async move {
            if let Err(e) = file_save_task.await {
                tracing::error!("File save task failed, Err: {e:?}");
            }
            drop(logfile_guard);
        });
        exit_cleanup_token_c.cancel();
    });

    inkview::iv_main(iv, move |event| {
        match event {
            event @ inkview::Event::Exit => {
                if msg_tx.clone().send(Msg::InkviewEvent(event)).is_err() {
                    tracing::error!("Failed to send InkviewEvent message, receiver closed.");
                }
                while !exit_cleanup_token.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(8));
                }
            }
            event => {
                if msg_tx.clone().send(Msg::InkviewEvent(event)).is_err() {
                    tracing::error!("Failed to send InkviewEvent message, receiver closed.");
                }
            }
        }
        Some(())
    });

    Ok(())
}

async fn setup_data_dir() -> anyhow::Result<()> {
    let data_dir = PathBuf::from(CLIENT_DATA_DIR);
    let cheatsheets_folder = data_dir.join(CHEATSHEETS_SUBFOLDER);
    let log_file_path = data_dir.join(LOG_FILE_NAME);

    if !data_dir.exists() {
        fs::create_dir(data_dir).await?;
    }
    if !cheatsheets_folder.exists() {
        fs::create_dir(cheatsheets_folder).await?;
    }
    if !log_file_path.exists() {
        fs::File::create(log_file_path).await?;
    }
    Ok(())
}

/// Returns a guard for log file writing that flushes any remaining logs when dropped.
fn setup_logging() -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let data_dir = PathBuf::from(CLIENT_DATA_DIR);

    let appender = tracing_appender::rolling::never(data_dir, LOG_FILE_NAME);
    let (file_appender, guard) = tracing_appender::non_blocking(appender);
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stdout.and(file_appender))
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    tracing::debug!("tracing initialized..");
    Ok(guard)
}

fn screen_orientation_iv_to_com(
    orientation: inkview::screen::ScreenOrientation,
) -> pb_cheatsheet_com::ScreenOrientation {
    match orientation {
        inkview::screen::ScreenOrientation::Portrait0Deg => {
            pb_cheatsheet_com::ScreenOrientation::Portrait0Deg
        }
        inkview::screen::ScreenOrientation::Landscape90Deg => {
            pb_cheatsheet_com::ScreenOrientation::Landscape90Deg
        }
        inkview::screen::ScreenOrientation::Portrait180Deg => {
            pb_cheatsheet_com::ScreenOrientation::Portrait180Deg
        }
        inkview::screen::ScreenOrientation::Landscape270Deg => {
            pb_cheatsheet_com::ScreenOrientation::Landscape270Deg
        }
    }
}
