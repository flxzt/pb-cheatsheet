pub(crate) mod dbus;
pub(crate) mod imageprocessing;

use anyhow::{anyhow, Context};
use clap::Parser;
use core::net::SocketAddr;
use imageprocessing::Rotate;
use pb_cheatsheet_com::{FocusedWindowInfo, TagsEither, WorldClient};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tarpc::context::Context as TarpcContext;
use tarpc::tokio_serde::formats::Json;
use tarpc::{client, context};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

/// pb-cheatsheet-host
///
/// To be used together with the client application on a pocketbook device{n}
/// to display cheatsheet's (images) depending on the current focused window.
#[derive(Debug, clap::Parser)]
struct Cli {
    /// The RPC server address of the client application.
    #[arg(short = 'a', long, env = "PB_CHEATSHEET_RPC_ADDR")]
    rpc_addr: String,
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Continuously report focused window info to the client.{n}
    /// Intended to be run as a service.
    ReportFocusedWindow,
    /// Get information stored on device.
    GetInfo,
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

pub fn long_rpc_context() -> TarpcContext {
    let mut context = context::current();
    context.deadline = Instant::now() + Duration::from_secs(60);
    context
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_tracing()?;
    let cli = Cli::parse();
    let quit_token = tokio_util::sync::CancellationToken::new();
    let dbus_connection = zbus::Connection::session().await?;
    let server_addr: SocketAddr = cli.rpc_addr.parse()?;
    let mut transport = tarpc::serde_transport::tcp::connect(server_addr, Json::default);
    println!("Connecting to RPC server with address: '{server_addr:?}'");
    transport.config_mut().max_frame_length(usize::MAX);
    let rpc_client = WorldClient::new(client::Config::default(), transport.await?).spawn();

    // Ctrl-C quit task
    let quit_token_c = quit_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Awaited ctrl_c signal");
        println!("Ctrl+C received, cancelling..");
        quit_token_c.cancel();
    });

    match cli.cmd {
        Command::ReportFocusedWindow => {
            run_report_focused_window(dbus_connection, rpc_client, quit_token.clone()).await;
        }
        Command::GetInfo => {
            run_get_info(rpc_client, quit_token).await?;
        }
        Command::UploadCheatsheet { image, name, tags } => {
            upload_cheatsheet_image(
                rpc_client,
                quit_token.clone(),
                image,
                name,
                tags.into_iter().collect(),
            )
            .await?;
        }
        Command::RemoveCheatsheet { name } => {
            run_remove_cheatsheet(rpc_client, quit_token, name).await?;
        }
        Command::Screenshot { name, invert } => {
            run_upload_screenshot(rpc_client, quit_token.clone(), name, invert).await?;
        }
        Command::ClearScreenshot => {
            run_clear_screenshot(rpc_client, quit_token).await?;
        }
        Command::AddCheatsheetTags { name, tags } => {
            run_add_cheatsheet_tags(rpc_client, quit_token, name, tags.into_iter().collect())
                .await?;
        }
        Command::RemoveCheatsheetTags { name, tags, all } => {
            let either = if all {
                TagsEither::All
            } else {
                TagsEither::Tags(tags.into_iter().collect())
            };
            run_remove_cheatsheet_tags(rpc_client, quit_token, name, either).await?;
        }
        Command::AddWmClassTags { wm_class, tags } => {
            run_add_wm_class_tags(rpc_client, quit_token, wm_class, tags.into_iter().collect())
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
            run_remove_wm_class_tags(rpc_client, quit_token, wm_class, either).await?;
        }
    }

    Ok(())
}

fn setup_tracing() -> Result<(), tracing::dispatcher::SetGlobalDefaultError> {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive("pb_cheatsheet_host=warn".parse().unwrap())
                .from_env_lossy(),
        )
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    debug!("tracing initialized..");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_report_focused_window(
    dbus_connection: zbus::Connection,
    rpc_client: WorldClient,
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
                println!("Reporting focused window change:\n{info:#?}");
                debug!("Sending focused window change..");
                if focused_window_tx.send(info.clone()).is_err() {
                    error!("Send changed focused window info to RPC client task, receiving side closed.");
                    quit_token_c.cancel();
                    break;
                }
                last_info = info;
                debug!("Sent focused window change");
            }
        }
    });

    // RPC client task
    let quit_token_c = quit_token.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = focus_window_rx.changed() => {
                    let info = focus_window_rx.borrow_and_update().clone();
                    if let Err(e) = rpc_client.focused_window(context::current(), info).await {
                        error!("Report focused window info over RPC, Err: {e:?}");
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
async fn run_get_info(
    rpc_client: WorldClient,
    _quit_token: CancellationToken,
) -> anyhow::Result<()> {
    let info = rpc_client.get_info(context::current()).await?;
    println!(
        "\nscreen width: {}, height: {}, orientation: {}",
        info.screen_width, info.screen_height, info.screen_orientation
    );

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
    rpc_client: WorldClient,
    quit_token: CancellationToken,
    image: PathBuf,
    name: String,
    tags: HashSet<String>,
) -> anyhow::Result<()> {
    let screen_info = rpc_client.get_info(context::current()).await?;
    debug!("Preparing cheatsheet image");
    let image = tokio::select! {
        image = imageprocessing::load_prepare_image(image, screen_info.screen_width, screen_info.screen_height, Rotate::Rotate0Deg, false) => {
            image.context("Load and prepare image from file")?
        },
        _ = quit_token.cancelled() => return Ok(())
    };

    println!("Uploading image..");
    tokio::select! {
        res = rpc_client.upload_cheatsheet(long_rpc_context(), image, name, tags) => {
            res.context("Upload image to client")?
        }
        _ = quit_token.cancelled() => return Ok(())
    }
    println!("Upload finished.");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_remove_cheatsheet(
    rpc_client: WorldClient,
    _quit_token: CancellationToken,
    name: String,
) -> anyhow::Result<()> {
    rpc_client
        .remove_cheatsheet(context::current(), name)
        .await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_upload_screenshot(
    rpc_client: WorldClient,
    quit_token: CancellationToken,
    name: Option<String>,
    invert: bool,
) -> anyhow::Result<()> {
    let screenshot_req = ashpd::desktop::screenshot::Screenshot::request()
        .interactive(true)
        .modal(true)
        .send();
    let screenshot = tokio::select! {
        response = screenshot_req => {
            let response = response?.response()?;
            let uri = response.uri().to_string();
            url::Url::parse(&uri)?.to_file_path().map_err(|err| anyhow!("Unable to convert screenshot URI to file path: {err:?}"))?
        },
        _ = quit_token.cancelled() => return Ok(())
    };
    debug!("Got screenshot path '{:?}'", screenshot);

    let screen_info = rpc_client.get_info(context::current()).await?;
    debug!("Preparing screenshot");
    let image = tokio::select! {
        image = imageprocessing::load_prepare_image(screenshot, screen_info.screen_width, screen_info.screen_height, Rotate::Rotate270Deg, invert) => {
            image.context("Load and prepare screenshot from file")?
        },
        _ = quit_token.cancelled() => return Ok(())
    };

    println!("Uploading screenshot..");
    tokio::select! {
        res = rpc_client.upload_screenshot(long_rpc_context(), image, name) => {
            res.context("Upload screenshot to client")?
        }
        _ = quit_token.cancelled() => return Ok(())
    }
    println!("Upload finished.");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_clear_screenshot(
    rpc_client: WorldClient,
    _quit_token: CancellationToken,
) -> anyhow::Result<()> {
    rpc_client.clear_screenshot(context::current()).await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_add_cheatsheet_tags(
    rpc_client: WorldClient,
    _quit_token: CancellationToken,
    name: String,
    tags: HashSet<String>,
) -> anyhow::Result<()> {
    rpc_client
        .add_cheatsheet_tags(context::current(), name, tags)
        .await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_remove_cheatsheet_tags(
    rpc_client: WorldClient,
    _quit_token: CancellationToken,
    name: String,
    either: TagsEither,
) -> anyhow::Result<()> {
    rpc_client
        .remove_cheatsheet_tags(context::current(), name, either)
        .await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_add_wm_class_tags(
    rpc_client: WorldClient,
    _quit_token: CancellationToken,
    wm_class: String,
    tags: HashSet<String>,
) -> anyhow::Result<()> {
    rpc_client
        .add_wm_class_tags(context::current(), wm_class, tags)
        .await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn run_remove_wm_class_tags(
    rpc_client: WorldClient,
    _quit_token: CancellationToken,
    wm_class: String,
    either: TagsEither,
) -> anyhow::Result<()> {
    rpc_client
        .remove_wm_class_tags(context::current(), wm_class, either)
        .await?;
    Ok(())
}
