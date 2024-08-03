pub(crate) mod dbus;
pub(crate) mod imageprocessing;

use anyhow::Context;
use clap::Parser;
use imageprocessing::Rotate;
use pb_cheatsheet_com::grpc::PbCheatsheetClientStruct;
use pb_cheatsheet_com::{FocusedWindowInfo, TagsEither};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

/// pb-cheatsheet-host
///
/// To be used together with the client application on a pocketbook device{n}
/// to display cheatsheet's (images) depending on the current focused window.
#[derive(Debug, clap::Parser)]
struct Cli {
    /// The GRPC server address of the client application.
    #[arg(short = 'a', long, env)]
    pb_grpc_addr: String,
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Continuously report focused window info to the client.{n}
    /// Intended to be run as a service.
    ReportFocusedWindow,
    /// Get device screen info.
    GetScreenInfo,
    /// Get cheatsheets info.
    GetCheatsheetsInfo,
    /// Upload a new chaetsheet that gets displayed when the added tags match the tags{n}
    /// that are added to the wm class of the reported window.{n}
    /// The image size is adjusted depending on the reported screen info of the client.
    UploadCheatsheet {
        /// The cheatsheet name.
        #[arg(short, long)]
        name: String,
        /// Associated tags.
        #[arg(short, long)]
        tags: Vec<String>,
        /// Path to the image
        image: PathBuf,
    },
    /// Remove a cheatsheet.
    RemoveCheatsheet {
        /// The cheatsheet name.
        name: String,
    },
    /// Take a screenshot and upload it to the device for transient display.
    Screenshot {
        /// An optional screenshot name.
        #[arg(short, long)]
        name: Option<String>,
        /// Whether the image colors should be inverted.
        #[arg(short, long)]
        invert: bool,
    },
    /// Clear the screenshot.
    ClearScreenshot,
    /// Add cheatsheet tags.
    AddCheatsheetTags {
        /// The cheatsheet name.
        #[arg(short, long)]
        name: String,
        /// Associated tags.
        #[arg(short, long)]
        tags: Vec<String>,
    },
    /// Remove cheatsheet tags.
    RemoveCheatsheetTags {
        /// The cheatsheet name.
        #[arg(short, long)]
        name: String,
        /// Associated tags.
        #[arg(short, long)]
        tags: Vec<String>,
        #[arg(short, long)]
        all: bool,
    },
    /// Add wm class tags.
    AddWmClassTags {
        /// The wm class.
        #[arg(short, long)]
        wm_class: String,
        /// Associated tags.
        #[arg(short, long)]
        tags: Vec<String>,
    },
    /// Remove wm class tags.
    RemoveWmClassTags {
        /// The wm class.
        #[arg(short, long)]
        wm_class: String,
        /// Associated tags.
        #[arg(short, long)]
        tags: Vec<String>,
        #[arg(short, long)]
        all: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_logging()?;
    let cli = Cli::parse();
    let quit_token = tokio_util::sync::CancellationToken::new();
    let dbus_connection = zbus::Connection::session().await?;
    println!(
        "Connecting to GRPC server with address: '{}'",
        cli.pb_grpc_addr
    );
    let grpc_client =
        pb_cheatsheet_com::grpc::PbCheatsheetClientStruct::new(&cli.pb_grpc_addr).await?;

    // Ctrl-C quit task
    let quit_token_c = quit_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Awaited ctrl_c signal");
        quit_token_c.cancel();
    });

    match cli.cmd {
        Command::ReportFocusedWindow => {
            run_report_focused_window(dbus_connection, grpc_client, quit_token.clone()).await;
        }
        Command::GetScreenInfo => {
            run_get_screen_info(grpc_client, quit_token).await?;
        }
        Command::GetCheatsheetsInfo => {
            run_get_cheatsheets_info(grpc_client, quit_token).await?;
        }
        Command::UploadCheatsheet { image, name, tags } => {
            upload_cheatsheet_image(
                grpc_client,
                quit_token.clone(),
                image,
                name,
                tags.into_iter().collect(),
            )
            .await?;
        }
        Command::RemoveCheatsheet { name } => {
            run_remove_cheatsheet(grpc_client, quit_token, name).await?;
        }
        Command::Screenshot { name, invert } => {
            run_upload_screenshot(
                grpc_client,
                quit_token.clone(),
                name.unwrap_or_default(),
                invert,
            )
            .await?;
        }
        Command::ClearScreenshot => {}
        Command::AddCheatsheetTags { name, tags } => {
            run_add_cheatsheet_tags(grpc_client, quit_token, name, tags.into_iter().collect())
                .await?;
        }
        Command::RemoveCheatsheetTags { name, tags, all } => {
            let either = if all {
                TagsEither::All
            } else {
                TagsEither::Tags(tags.into_iter().collect())
            };
            run_remove_cheatsheet_tags(grpc_client, quit_token, name, either).await?;
        }
        Command::AddWmClassTags { wm_class, tags } => {
            run_add_wm_class_tags(
                grpc_client,
                quit_token,
                wm_class,
                tags.into_iter().collect(),
            )
            .await?;
        }
        Command::RemoveWmClassTags {
            wm_class,
            tags,
            all,
        } => {
            let either = if all {
                TagsEither::All
            } else {
                TagsEither::Tags(tags.into_iter().collect())
            };
            run_remove_wm_class_tags(grpc_client, quit_token, wm_class, either).await?;
        }
    }

    Ok(())
}

fn setup_logging() -> Result<(), tracing::dispatcher::SetGlobalDefaultError> {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    debug!("tracing initialized..");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_report_focused_window(
    dbus_connection: zbus::Connection,
    mut grpc_client: PbCheatsheetClientStruct,
    quit_token: CancellationToken,
) {
    let (focused_window_tx, mut focus_window_rx) =
        tokio::sync::watch::channel::<FocusedWindowInfo>(FocusedWindowInfo::default());

    // focused window D-Bus poll task
    let quit_token_c = quit_token.clone();
    tokio::task::spawn(async move {
        let mut poll_interval = tokio::time::interval(Duration::from_millis(1000));
        let mut last_info = match dbus::get_focused_window_info(&dbus_connection).await {
            Ok(i) => i,
            Err(e) => {
                error!("Get initial focused window info failed, aborting application. Err: {e:?}");
                quit_token_c.cancel();
                return;
            }
        };

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {},
                _ = quit_token_c.cancelled() => break,
            }
            let info = match dbus::get_focused_window_info(&dbus_connection).await {
                Ok(i) => i,
                Err(e) => {
                    error!("Poll focused window info from D-Bus, Err: {e:?}");
                    continue;
                }
            };
            if info != last_info {
                debug!("Got focused window change:\n{info:#?}");
                if focused_window_tx.send(info.clone()).is_err() {
                    error!("Send changed focused window info to GRPC client task, receiving side closed.");
                    quit_token_c.cancel();
                    break;
                }
                last_info = info;
            }
        }
    });

    // GRPC client task
    let quit_token_c = quit_token.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = focus_window_rx.changed() => {
                    let info = focus_window_rx.borrow_and_update().clone();
                    if let Err(e) = grpc_client.focused_window(info).await {
                        error!("Report focused window info over GRPC, Err: {e:?}");
                    }
                },
                _ = quit_token_c.cancelled() => break
            }
        }
    });

    quit_token.cancelled().await;
    println!("Exiting..");
}

#[tracing::instrument(skip_all)]
async fn run_get_screen_info(
    mut grpc_client: PbCheatsheetClientStruct,
    _quit_token: CancellationToken,
) -> anyhow::Result<()> {
    let info = grpc_client.get_screen_info().await?;
    println!("Got screen info:\n{info:#?}");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_get_cheatsheets_info(
    mut grpc_client: PbCheatsheetClientStruct,
    _quit_token: CancellationToken,
) -> anyhow::Result<()> {
    let info = grpc_client.get_cheatsheets_info().await?;
    println!("\ncheatsheets tags:");
    for sheet_tags in info.cheatsheets.iter() {
        let n_tags = sheet_tags.tags.len();

        print!("  {} : [", sheet_tags.name);
        for (i, tag) in sheet_tags.tags.iter().enumerate() {
            if i > 0 && i <= n_tags.saturating_sub(1) {
                print!(", ")
            }
            print!("{tag}");
        }
        println!("]");
    }
    println!("\nwm classes tags:");
    for wm_class_tags in info.wm_classes.iter() {
        let n_tags = wm_class_tags.tags.len();

        print!("  {} : [", wm_class_tags.wm_class);
        for (i, tag) in wm_class_tags.tags.iter().enumerate() {
            if i > 0 && i <= n_tags.saturating_sub(1) {
                print!(", ")
            }
            print!("{tag}");
        }
        println!("]");
    }
    println!();
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn upload_cheatsheet_image(
    mut grpc_client: PbCheatsheetClientStruct,
    quit_token: CancellationToken,
    image: PathBuf,
    name: String,
    tags: HashSet<String>,
) -> anyhow::Result<()> {
    let screen_info = grpc_client.get_screen_info().await?;
    debug!("Preparing image for device with fetched screen info: '{screen_info:#?}'");
    let image = tokio::select! {
        image = imageprocessing::load_prepare_image(image, screen_info.width, screen_info.height, Rotate::Rotate0Deg, false) => {
            image.context("Load and prepare image from file")?
        },
        _ = quit_token.cancelled() => return Ok(())
    };
    debug!("Uploading image..");
    tokio::select! {
        res = grpc_client.upload_cheatsheet_image(image, name, tags) => {
            res.context("Upload image to client")?
        }
        _ = quit_token.cancelled() => return Ok(())
    }
    debug!("Upload finished.");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_remove_cheatsheet(
    mut grpc_client: PbCheatsheetClientStruct,
    _quit_token: CancellationToken,
    name: String,
) -> anyhow::Result<()> {
    grpc_client.remove_cheatsheet(name).await
}

#[tracing::instrument(skip_all)]
async fn run_upload_screenshot(
    mut grpc_client: PbCheatsheetClientStruct,
    quit_token: CancellationToken,
    name: String,
    invert: bool,
) -> anyhow::Result<()> {
    let screenshot_req = ashpd::desktop::screenshot::Screenshot::request()
        .interactive(true)
        .modal(true)
        .send();
    let screenshot = tokio::select! {
        response = screenshot_req => {
            let response = response?.response()?;
            response.uri().to_file_path().map_err(|e| anyhow::anyhow!("Retrieving file path from returned screenshot URL, Err: {e:?}"))?
        },
        _ = quit_token.cancelled() => return Ok(())
    };

    debug!("Got screenshot path '{:?}'", screenshot);

    let screen_info = grpc_client.get_screen_info().await?;
    debug!("Preparing screenshot for device with fetched screen info: '{screen_info:#?}'");
    let image = tokio::select! {
        image = imageprocessing::load_prepare_image(screenshot, screen_info.width, screen_info.height, Rotate::Rotate270Deg, invert) => {
            image.context("Load and prepare screenshot from file")?
        },
        _ = quit_token.cancelled() => return Ok(())
    };
    debug!("Uploading screenshot..");
    tokio::select! {
        res = grpc_client.upload_screenshot(image, name) => {
            res.context("Upload screenshot to client")?
        }
        _ = quit_token.cancelled() => return Ok(())
    }
    debug!("Upload finished.");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_clear_screenshot(
    mut grpc_client: PbCheatsheetClientStruct,
    quit_token: CancellationToken,
) -> anyhow::Result<()> {
    tokio::select! {
        res = grpc_client.clear_screenshot() => res,
        _ = quit_token.cancelled() => return Ok(())
    }
}

#[tracing::instrument(skip_all)]
async fn run_add_cheatsheet_tags(
    mut grpc_client: PbCheatsheetClientStruct,
    _quit_token: CancellationToken,
    name: String,
    tags: HashSet<String>,
) -> anyhow::Result<()> {
    grpc_client.add_cheatsheet_tags(name, tags).await
}

#[tracing::instrument(skip_all)]
async fn run_remove_cheatsheet_tags(
    mut grpc_client: PbCheatsheetClientStruct,
    _quit_token: CancellationToken,
    name: String,
    either: TagsEither,
) -> anyhow::Result<()> {
    grpc_client.remove_cheatsheet_tags(name, either).await
}

#[tracing::instrument(skip_all)]
async fn run_add_wm_class_tags(
    mut grpc_client: PbCheatsheetClientStruct,
    _quit_token: CancellationToken,
    wm_class: String,
    tags: HashSet<String>,
) -> anyhow::Result<()> {
    grpc_client.add_wm_class_tags(wm_class, tags).await
}

#[tracing::instrument(skip_all)]
async fn run_remove_wm_class_tags(
    mut grpc_client: PbCheatsheetClientStruct,
    _quit_token: CancellationToken,
    wm_class: String,
    either: TagsEither,
) -> anyhow::Result<()> {
    grpc_client.remove_wm_class_tags(wm_class, either).await
}
