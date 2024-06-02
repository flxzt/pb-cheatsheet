use pb_cheatsheet_com::grpc::PbCheatsheetServerImpl;
use pb_cheatsheet_com::{CheatsheetImage, CheatsheetsInfo, FocusedWindowInfo, ScreenInfo};
use std::collections::HashSet;

struct GrpcServer {}

#[async_trait::async_trait]
impl PbCheatsheetServerImpl for GrpcServer {
    async fn handle_focused_window(&self, info: FocusedWindowInfo) {
        println!("Received focused window");
        println!("{info:#?}");
    }

    async fn handle_get_screen_info(&self) -> anyhow::Result<ScreenInfo> {
        println!("Received get screen info request");
        Ok(ScreenInfo {
            width: 1920,
            height: 1080,
            orientation: pb_cheatsheet_com::ScreenOrientation::default(),
        })
    }

    async fn handle_get_cheatsheets_info(&self) -> anyhow::Result<CheatsheetsInfo> {
        println!("Received get cheatsheets info");
        Ok(CheatsheetsInfo::default())
    }

    async fn handle_upload_cheatsheet(
        &self,
        image: CheatsheetImage,
        name: String,
        tags: HashSet<String>,
    ) {
        println!("Received upload cheatsheet");
        println!("{image:#?}");
        println!("{name:#?}");
        println!("{tags:#?}");
    }

    async fn handle_remove_cheatsheet(&self, name: String) {
        println!("Received remove cheatsheet");
        println!("{name:#?}");
    }

    async fn handle_add_cheatsheet_tags(&self, name: String, tags: HashSet<String>) {
        println!("Received add cheatsheet tags");
        println!("{name:#?}");
        println!("{tags:#?}");
    }

    async fn handle_remove_cheatsheet_tags(&self, name: String, tags: HashSet<String>) {
        println!("Received remove cheatsheet tags");
        println!("{name:#?}");
        println!("{tags:#?}");
    }

    async fn handle_add_wm_class_tags(&self, wm_class: String, tags: HashSet<String>) {
        println!("Received add wm class tags");
        println!("{wm_class:#?}");
        println!("{tags:#?}");
    }

    async fn handle_remove_wm_class_tags(&self, wm_class: String, tags: HashSet<String>) {
        println!("Received remove wm class tags");
        println!("{wm_class:#?}");
        println!("{tags:#?}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    const SERVER_ADDR: &str = "0.0.0.0:51151";
    let quit_token = tokio_util::sync::CancellationToken::new();

    // Ctrl-C cancel task
    let quit_token_c = quit_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Awaited ctrl_c signal");
        quit_token_c.cancel();
    });

    println!("Starting GRPC server with listening address: '{SERVER_ADDR}'");
    tokio::select! {
        _ = pb_cheatsheet_com::grpc::start_server(GrpcServer {}, SERVER_ADDR.parse().unwrap()) => {}
        _ = quit_token.cancelled() => {}
    }

    println!("Exiting..");
    Ok(())
}
