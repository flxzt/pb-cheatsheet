pub(crate) mod cheatsheets;
pub(crate) mod wifi;

use anyhow::Context;
use cheatsheets::{Cheatsheet, Cheatsheets};
use core::convert::Infallible;
use core::fmt::Display;
use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_9X15};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{self, Gray8};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, StyledDrawable};
use embedded_graphics::text::Text;
use futures::{future, prelude::*};
use inkview::bindings::Inkview;
use inkview_eg::InkviewDisplay;
use pb_cheatsheet_com::{
    CheatsheetImage, FocusedWindowInfo, Info, ScreenOrientation, TagsEither, World, RPC_PORT,
};
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use tarpc::context::Context as TarpcContext;
use tarpc::server::{self, Channel};
use tarpc::tokio_serde::formats::Json;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;
use tracing::{debug, error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::writer::MakeWriterExt;

const CLIENT_DATA_DIR: &str = "/mnt/ext1/applications/pb-cheatsheet-data";
const CHEATSHEETS_SUBFOLDER: &str = "cheatsheets";
const LOG_FILE_NAME: &str = "pb-cheatsheet.log";

#[derive(Debug)]
enum Msg {
    InkviewEvent(inkview::Event),
    FocusedWindow(FocusedWindowInfo),
    GetInfo(oneshot::Sender<Info>),
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
        name: Option<String>,
    },
    ClearScreenshot,
    AddCheatsheetTags {
        name: String,
        tags: HashSet<String>,
    },
    RemoveCheatsheetTags {
        name: String,
        either: TagsEither,
    },
    AddWmClassTags {
        wm_class: String,
        tags: HashSet<String>,
    },
    RemoveWmClassTags {
        wm_class: String,
        either: TagsEither,
    },
}

#[derive(Debug, Clone)]
struct TarpcServer {
    #[allow(unused)]
    peer_addr: SocketAddr,
    msg_tx: mpsc::UnboundedSender<Msg>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum UiMode {
    /// Switch manually through all cheatsheets
    Manual,
    /// Automatic cheatsheet page switching dependending on matched tags based on the current reported wm class
    #[default]
    AutomaticWmClass,
    /// Screenshot
    Screenshot,
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
    pub screen_width: u32,
    pub screen_height: u32,
    pub screen_orientation: ScreenOrientation,
    pub cheatsheets: Cheatsheets,
    /// Current pages for wm class
    pub current_page: HashMap<String, usize>,
    pub manual_mode_current_page: usize,
    pub screenshot: Option<(Cheatsheet, Option<String>)>,
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
            screen_width: 0,
            screen_height: 0,
            screen_orientation: ScreenOrientation::Portrait0Deg,
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
        self.screen_width = display.iv_screen_ref().width() as u32;
        self.screen_height = display.iv_screen_ref().height() as u32;
        self.screen_orientation =
            screen_orientation_iv_to_com(display.iv_screen_ref().orientation());
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
                self.screen_width,
                self.screen_height,
                self.screen_orientation,
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

impl World for TarpcServer {
    async fn focused_window(self, _: TarpcContext, info: FocusedWindowInfo) {
        if self.msg_tx.send(Msg::FocusedWindow(info)).is_err() {
            error!(
                "Sending received RPC focused window info from handler failed, receiving half closed"
            );
        }
    }

    async fn get_info(self, _: TarpcContext) -> Info {
        let (tx, rx) = oneshot::channel();
        if self.msg_tx.send(Msg::GetInfo(tx)).is_err() {
            error!(
                "Sending received RPC get info sender from handler failed, receiving half closed"
            );
        }
        let Ok(info) = rx.await else {
            error!("Receiving request info failed, sender half dropped");
            return Info::default();
        };
        info
    }

    async fn upload_cheatsheet(
        self,
        _: TarpcContext,
        image: CheatsheetImage,
        name: String,
        tags: HashSet<String>,
    ) {
        if self
            .msg_tx
            .send(Msg::UploadCheatsheet { image, name, tags })
            .is_err()
        {
            error!(
                "Sending received RPC cheatsheet image from handler failed, receiving half closed"
            );
        }
    }

    async fn remove_cheatsheet(self, _: TarpcContext, name: String) {
        if self.msg_tx.send(Msg::RemoveCheatsheet { name }).is_err() {
            error!("Sending remove cheatsheet message failed, receiving half closed");
        }
    }

    async fn upload_screenshot(
        self,
        _: TarpcContext,
        screenshot: CheatsheetImage,
        name: Option<String>,
    ) {
        if self
            .msg_tx
            .send(Msg::UploadScreenshot { screenshot, name })
            .is_err()
        {
            error!(
                "Sending received RPC cheatsheet image from handler failed, receiving half closed"
            );
        }
    }

    async fn clear_screenshot(self, _: TarpcContext) {
        if self.msg_tx.send(Msg::ClearScreenshot).is_err() {
            error!("Sending received RPC screenshot from handler failed, receiving half closed");
        }
    }

    async fn add_cheatsheet_tags(self, _: TarpcContext, name: String, tags: HashSet<String>) {
        if self
            .msg_tx
            .send(Msg::AddCheatsheetTags { name, tags })
            .is_err()
        {
            error!("Sending add cheatsheet tags message failed, receiving half closed");
        }
    }

    async fn remove_cheatsheet_tags(self, _: TarpcContext, name: String, either: TagsEither) {
        if self
            .msg_tx
            .send(Msg::RemoveCheatsheetTags { name, either })
            .is_err()
        {
            error!("Sending remove cheatsheet tags message failed, receiving half closed");
        }
    }

    async fn add_wm_class_tags(self, _: TarpcContext, wm_class: String, tags: HashSet<String>) {
        if self
            .msg_tx
            .send(Msg::AddWmClassTags { wm_class, tags })
            .is_err()
        {
            error!("Sending add wm class tags message failed, receiving half closed");
        }
    }

    async fn remove_wm_class_tags(self, _: TarpcContext, wm_class: String, either: TagsEither) {
        if self
            .msg_tx
            .send(Msg::RemoveWmClassTags { wm_class, either })
            .is_err()
        {
            error!("Sending add wm class tags message failed, receiving half closed");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Debugging
    std::env::set_var("RUST_BACKTRACE", "1");

    setup_data_dir().await?;
    let logfile_guard = setup_tracing()?;
    let iv: &'static inkview::bindings::Inkview = Box::leak(Box::new(inkview::load())) as &_;
    // The cancel token is used to indicated when the app should be quit
    let quit_token = tokio_util::sync::CancellationToken::new();
    // The exit cleanup token is used to block the main loop while cleanup tasks are running.
    let exit_cleanup_token = tokio_util::sync::CancellationToken::new();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<Msg>();
    let (file_save_tx, file_save_rx) = mpsc::unbounded_channel::<(PathBuf, Vec<u8>)>();

    // File save task
    let file_save_task = tokio::spawn(async move {
        if let Err(err) = spawn_file_save_task(file_save_rx).await {
            error!("File save task failed: {err:?}");
        }
    });

    // RPC server task
    let quit_token_c = quit_token.clone();
    let msg_tx_c = msg_tx.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = spawn_rpc_server(msg_tx_c) => {}
            _ = quit_token_c.cancelled() => {}
        }
    });

    // Msg handle task
    let exit_cleanup_token_c = exit_cleanup_token.clone();
    tokio::task::spawn_blocking(move || {
        spawn_msg_handler_task(
            iv,
            msg_rx,
            file_save_tx,
            file_save_task,
            logfile_guard,
            quit_token,
            exit_cleanup_token_c,
        )
    });

    inkview::iv_main(iv, move |event| {
        match event {
            event @ inkview::Event::Exit => {
                if msg_tx.clone().send(Msg::InkviewEvent(event)).is_err() {
                    error!("Failed to send InkviewEvent message, receiver closed.");
                }
                while !exit_cleanup_token.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            event => {
                if msg_tx.clone().send(Msg::InkviewEvent(event)).is_err() {
                    error!("Failed to send InkviewEvent message, receiver closed.");
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
fn setup_tracing() -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let data_dir = PathBuf::from(CLIENT_DATA_DIR);

    let appender = tracing_appender::rolling::never(data_dir, LOG_FILE_NAME);
    let (file_appender, guard) = tracing_appender::non_blocking(appender);
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_writer(std::io::stdout.and(file_appender))
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    debug!("tracing initialized..");
    Ok(guard)
}

async fn spawn_file_save_task(
    mut file_save_rx: UnboundedReceiver<(PathBuf, Vec<u8>)>,
) -> anyhow::Result<()> {
    while let Some((file_path, data)) = file_save_rx.recv().await {
        debug!("Saving file with path '{}'", file_path.display());
        if let Err(err) = async {
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
            error!(
                "Saving file '{}' failed, Err: {err:?}'",
                file_path.display()
            );
        }
    }
    debug!("File save task finished, sender closed");
    Ok(())
}

async fn spawn_rpc_server(msg_tx: UnboundedSender<Msg>) -> anyhow::Result<()> {
    let server_addr = (Ipv4Addr::new(0, 0, 0, 0), RPC_PORT);
    let mut listener = tarpc::serde_transport::tcp::listen(&server_addr, Json::default).await?;
    info!("Started RPC server with listening address: '{server_addr:?}'");
    listener.config_mut().max_frame_length(usize::MAX);
    listener
        // Ignore accept errors.
        .filter_map(|r| future::ready(r.ok()))
        .map(server::BaseChannel::with_defaults)
        // serve is generated by the service attribute. It takes as input any type implementing
        // the generated World trait.
        .map(|channel| {
            let server = TarpcServer {
                peer_addr: channel.transport().peer_addr().unwrap(),
                msg_tx: msg_tx.clone(),
            };
            channel.execute(server.serve()).for_each(|fut| async {
                tokio::spawn(fut);
            })
        })
        // Max 10 channels.
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;
    Ok(())
}

async fn spawn_signal_handler_task(quit_token: CancellationToken) -> anyhow::Result<()> {
    let mut stream_sigquit = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::quit())
        .context("Create SIGQUIT signal stream")?;
    let mut stream_sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .context("Create SIGTERM signal stream")?;
    let mut stream_sigint =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .context("Create SIGINT signal stream")?;
    let mut stream_sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .expect("Create SIGHUP signal stream");
    tokio::select! {
        _ = stream_sigquit.recv() => {},
        _ = stream_sigterm.recv() => {},
        _ = stream_sigint.recv() => {},
        _ = stream_sighup.recv() => {},
    }
    quit_token.cancel();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn spawn_msg_handler_task(
    iv: &'static inkview::bindings::Inkview,
    mut msg_rx: UnboundedReceiver<Msg>,
    file_save_tx: UnboundedSender<(PathBuf, Vec<u8>)>,
    file_save_task: JoinHandle<()>,
    logfile_guard: WorkerGuard,
    quit_token: CancellationToken,
    exit_cleanup_token: CancellationToken,
) {
    let mut ui_state = tokio::runtime::Handle::current().block_on(async move {
        UiState::with_loaded_cheatsheets()
            .await
            .inspect_err(|e| error!("Display state image loading failed, Err: {e:?}"))
            .unwrap_or_default()
    });
    let mut display: OnceCell<InkviewDisplay> = OnceCell::new();

    loop {
        let mut repaint = false;
        let mut save_cheatsheets = false;
        let mut save_metadata = false;

        let msg = msg_rx.blocking_recv();
        debug!("Handling received message:\n{msg:?}");
        let Some(msg) = msg else {
            continue;
        };

        match msg {
            Msg::InkviewEvent(event) => {
                match event {
                    inkview::Event::Init => {
                        // UNIX signals quit task.
                        // *Must* be initialized after inkview's main,
                        // otherwise it will be overwritten
                        tokio::spawn(spawn_signal_handler_task(quit_token.clone()));

                        // Initialize display
                        if display.set(InkviewDisplay::new(iv)).is_err() {
                            error!("Inkview display was already initialized when inkview init event was received");
                        }

                        // Wifi keep-alive task
                        let quit_token_c = quit_token.clone();
                        tokio::spawn(async move {
                            let mut interval = tokio::time::interval(Duration::from_secs(30));

                            loop {
                                tokio::select! {
                                    _ = interval.tick() => {
                                        if let Err(e) = wifi::wifi_keepalive(iv) {
                                            error!("Wifi keep-alive failed, Err: {e:?}");
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
            Msg::GetInfo(tx) => {
                if let Some(display) = display.get() {
                    let size = display.size();
                    let orientation = display.iv_screen_ref().orientation();
                    let cheatsheets = ui_state.cheatsheets.get_sheet_tags();
                    let wm_classes = ui_state.cheatsheets.get_wm_classes_tags();
                    if tx
                        .send(Info {
                            screen_width: size.width,
                            screen_height: size.height,
                            screen_orientation: screen_orientation_iv_to_com(orientation),
                            cheatsheets,
                            wm_classes,
                        })
                        .is_err()
                    {
                        error!(
                            "Sending screen info answer over channel failed, receiver half dropped"
                        );
                    }
                } else {
                    warn!("Display not initialized yet when trying to retrieve its dimensions");
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

                repaint = true;
                save_cheatsheets = true;
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
                        error!("Failed to add tag to cheatsheet '{name}', Err: {e:?}");
                    }
                }
                repaint = true;
                save_metadata = true;
            }
            Msg::RemoveCheatsheetTags { name, either } => {
                match either {
                    TagsEither::Tags(tags) => {
                        for tag in tags {
                            if let Err(e) = ui_state.cheatsheets.remove_sheet_tag(&name, &tag) {
                                error!(
                                        "Failed to remove tag '{tag}' from cheatsheet '{name}', Err: {e:?}"
                                    );
                            }
                        }
                    }
                    TagsEither::All => {
                        if let Err(e) = ui_state.cheatsheets.clear_sheet_tags(&name) {
                            error!("Failed to remove clear cheatsheet '{name}' tags, Err: {e:?}");
                        }
                    }
                };
                repaint = true;
                save_metadata = true;
            }
            Msg::AddWmClassTags { wm_class, tags } => {
                for tag in tags {
                    ui_state.cheatsheets.add_wm_class_tag(&wm_class, tag);
                }
                repaint = true;
                save_metadata = true;
            }
            Msg::RemoveWmClassTags { wm_class, either } => {
                match either {
                    TagsEither::Tags(tags) => {
                        for tag in tags {
                            if let Err(e) =
                                ui_state.cheatsheets.remove_wm_class_tag(&wm_class, &tag)
                            {
                                error!(
                                        "Failed to remove tag '{tag}' from wm class '{wm_class}', Err: {e:?}"
                                    );
                            }
                        }
                    }
                    TagsEither::All => {
                        if let Err(e) = ui_state.cheatsheets.remove_wm_class(&wm_class) {
                            error!("Failed to remove clear wm class '{wm_class}' tags, Err: {e:?}");
                        }
                    }
                };
                repaint = true;
                save_metadata = true;
            }
        }

        if repaint {
            let Some(display) = display.get_mut() else {
                warn!("Display not initialized yet when trying to repaint.");
                continue;
            };
            ui_state.update(iv, display);

            if let Err(e) = ui_state.draw_to_display(display) {
                error!("Drawing display state failed, Err: {e:?}");
            }
            display.flush();
        }

        if save_cheatsheets {
            debug!("Saving cheatsheets and metadata");

            if let Err(e) = ui_state.cheatsheets.dispatch_save_all(
                PathBuf::from(CLIENT_DATA_DIR).join(CHEATSHEETS_SUBFOLDER),
                file_save_tx.clone(),
            ) {
                error!("Failed to dispatch saving cheatsheets, Err: {e:?}");
            };
        } else if save_metadata {
            debug!("Saving metadata");

            if let Err(e) = ui_state.cheatsheets.dispatch_save_metadata(
                PathBuf::from(CLIENT_DATA_DIR).join(CHEATSHEETS_SUBFOLDER),
                file_save_tx.clone(),
            ) {
                error!("Failed to dispatch saving metadata, Err: {e:?}");
            };
        }

        if quit_token.is_cancelled() {
            debug!("Quitting! Saving cheatsheets and metadata");

            if let Err(e) = ui_state.cheatsheets.dispatch_save_all(
                PathBuf::from(CLIENT_DATA_DIR).join(CHEATSHEETS_SUBFOLDER),
                file_save_tx,
            ) {
                error!("Failed to dispatch saving images on exit, Err: {e:?}");
            };

            break;
        }
    }

    info!("Exiting..");
    tokio::runtime::Handle::current().block_on(async move {
        if let Err(e) = file_save_task.await {
            error!("File save task failed, Err: {e:?}");
        }
        drop(logfile_guard);
    });
    exit_cleanup_token.cancel();
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
