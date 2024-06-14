pub mod pb_cheatsheet_proto {
    tonic::include_proto!("pb_cheatsheet");
}

use self::pb_cheatsheet_proto::pb_cheatsheet_client::PbCheatsheetClient;
use crate::{
    ByteOrder, CheatsheetImage, CheatsheetsInfo, FocusedWindowInfo, ImageFormat, ScreenInfo,
};
use anyhow::Context;
use pb_cheatsheet_proto::pb_cheatsheet_server::PbCheatsheet;
use pb_cheatsheet_proto::pb_cheatsheet_server::PbCheatsheetServer;
use std::collections::HashSet;
use std::net::SocketAddr;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Response, Status};

impl From<pb_cheatsheet_proto::FocusedWindowInfo> for FocusedWindowInfo {
    fn from(value: pb_cheatsheet_proto::FocusedWindowInfo) -> Self {
        Self {
            title: value.title,
            wm_class: value.wm_class,
            wm_class_instance: value.wm_class_instance,
            pid: value.pid,
            focus: value.focus,
        }
    }
}

impl From<FocusedWindowInfo> for pb_cheatsheet_proto::FocusedWindowInfo {
    fn from(value: FocusedWindowInfo) -> Self {
        Self {
            title: value.title,
            wm_class: value.wm_class,
            wm_class_instance: value.wm_class_instance,
            pid: value.pid,
            focus: value.focus,
        }
    }
}

impl From<pb_cheatsheet_proto::ImageFormat> for ImageFormat {
    fn from(value: pb_cheatsheet_proto::ImageFormat) -> Self {
        match value {
            pb_cheatsheet_proto::ImageFormat::Gray8 => Self::Gray8,
        }
    }
}

impl From<ImageFormat> for pb_cheatsheet_proto::ImageFormat {
    fn from(value: ImageFormat) -> Self {
        match value {
            ImageFormat::Gray8 => Self::Gray8,
        }
    }
}

impl From<pb_cheatsheet_proto::ByteOrder> for ByteOrder {
    fn from(value: pb_cheatsheet_proto::ByteOrder) -> Self {
        match value {
            pb_cheatsheet_proto::ByteOrder::Le => Self::LE,
            pb_cheatsheet_proto::ByteOrder::Be => Self::BE,
        }
    }
}

impl From<ByteOrder> for pb_cheatsheet_proto::ByteOrder {
    fn from(value: ByteOrder) -> Self {
        match value {
            ByteOrder::LE => Self::Le,
            ByteOrder::BE => Self::Be,
        }
    }
}

impl TryFrom<pb_cheatsheet_proto::ScreenInfo> for ScreenInfo {
    type Error = anyhow::Error;

    fn try_from(value: pb_cheatsheet_proto::ScreenInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            width: value.width,
            height: value.height,
            orientation: value.orientation.try_into()?,
        })
    }
}

impl TryFrom<ScreenInfo> for pb_cheatsheet_proto::ScreenInfo {
    type Error = anyhow::Error;

    fn try_from(value: ScreenInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            width: value.width,
            height: value.height,
            orientation: value.orientation.try_into()?,
        })
    }
}

#[async_trait::async_trait]
pub trait PbCheatsheetServerImpl: PbCheatsheet + Send + Sync + 'static {
    async fn handle_focused_window(&self, info: FocusedWindowInfo);
    async fn handle_get_screen_info(&self) -> anyhow::Result<ScreenInfo>;
    async fn handle_get_cheatsheets_info(&self) -> anyhow::Result<CheatsheetsInfo>;
    async fn handle_upload_cheatsheet(
        &self,
        image: CheatsheetImage,
        name: String,
        tags: HashSet<String>,
    );
    async fn handle_upload_screenshot(&self, screenshot: CheatsheetImage, name: String);
    async fn handle_clear_screenshot(&self);
    async fn handle_remove_cheatsheet(&self, name: String);
    async fn handle_add_cheatsheet_tags(&self, name: String, tags: HashSet<String>);
    async fn handle_remove_cheatsheet_tags(&self, name: String, tags: HashSet<String>);
    async fn handle_add_wm_class_tags(&self, wm_class: String, tags: HashSet<String>);
    async fn handle_remove_wm_class_tags(&self, wm_class: String, tags: HashSet<String>);
}

#[tonic::async_trait]
impl<S> PbCheatsheet for S
where
    S: PbCheatsheetServerImpl,
{
    async fn focused_window(
        &self,
        info: Request<pb_cheatsheet_proto::FocusedWindowInfo>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let info = info.into_inner().into();
        self.handle_focused_window(info).await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn get_screen_info(
        &self,
        _req: Request<pb_cheatsheet_proto::Empty>,
    ) -> Result<Response<pb_cheatsheet_proto::ScreenInfo>, Status> {
        let res = self
            .handle_get_screen_info()
            .await
            .map_err(|_e| Status::new(Code::Internal, "Getting screen info from server"))?;
        Ok(Response::new(res.try_into().map_err(|_e| {
            Status::new(
                Code::Internal,
                "Converting screen info to proto representation",
            )
        })?))
    }

    async fn get_cheatsheets_info(
        &self,
        _req: Request<pb_cheatsheet_proto::Empty>,
    ) -> Result<Response<pb_cheatsheet_proto::CheatsheetsInfo>, Status> {
        let res = self
            .handle_get_cheatsheets_info()
            .await
            .map_err(|_e| Status::new(Code::Internal, "Getting cheatsheets info from server"))?;
        Ok(Response::new(res.into()))
    }

    async fn upload_cheatsheet(
        &self,
        req: Request<pb_cheatsheet_proto::UploadCheatsheetRequest>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let req = req.into_inner();
        let name = req.name;
        let tags: HashSet<String> = req
            .tags
            .map(|t| t.tags.into_iter().collect())
            .unwrap_or_default();
        let format = ImageFormat::try_from(req.format)
            .map_err(|_e| Status::new(Code::Internal, "'ImageFormat' try from received i32"))?;
        let order = ByteOrder::try_from(req.order)
            .map_err(|_e| Status::new(Code::Internal, "'ByteOrder' try from received i32"))?;
        let width = req.width;
        let height = req.height;
        let data = req.image_data;
        self.handle_upload_cheatsheet(
            CheatsheetImage {
                format,
                order,
                width,
                height,
                data,
            },
            name,
            tags,
        )
        .await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn remove_cheatsheet(
        &self,
        req: Request<pb_cheatsheet_proto::RemoveCheatsheetRequest>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let req = req.into_inner();
        self.handle_remove_cheatsheet(req.name).await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn upload_screenshot(
        &self,
        req: Request<pb_cheatsheet_proto::UploadScreenshotRequest>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let req = req.into_inner();
        let name = req.name;
        let format = ImageFormat::try_from(req.format)
            .map_err(|_e| Status::new(Code::Internal, "'ImageFormat' try from received i32"))?;
        let order = ByteOrder::try_from(req.order)
            .map_err(|_e| Status::new(Code::Internal, "'ByteOrder' try from received i32"))?;
        let width = req.width;
        let height = req.height;
        let data = req.image_data;
        self.handle_upload_screenshot(
            CheatsheetImage {
                format,
                order,
                width,
                height,
                data,
            },
            name,
        )
        .await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn clear_screenshot(
        &self,
        _req: Request<pb_cheatsheet_proto::Empty>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        self.handle_clear_screenshot().await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn add_cheatsheet_tags(
        &self,
        req: Request<pb_cheatsheet_proto::AddCheatsheetTagsRequest>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let req = req.into_inner();
        self.handle_add_cheatsheet_tags(
            req.name,
            req.tags
                .map(|t| t.tags.into_iter().collect())
                .unwrap_or_default(),
        )
        .await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn remove_cheatsheet_tags(
        &self,
        req: Request<pb_cheatsheet_proto::RemoveCheatsheetTagsRequest>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let req = req.into_inner();
        self.handle_remove_cheatsheet_tags(
            req.name,
            req.tags
                .map(|t| t.tags.into_iter().collect())
                .unwrap_or_default(),
        )
        .await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn add_wm_class_tags(
        &self,
        req: Request<pb_cheatsheet_proto::AddWmClassTagsRequest>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let req = req.into_inner();
        self.handle_add_wm_class_tags(
            req.wm_class,
            req.tags
                .map(|t| t.tags.into_iter().collect())
                .unwrap_or_default(),
        )
        .await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }

    async fn remove_wm_class_tags(
        &self,
        req: Request<pb_cheatsheet_proto::RemoveWmClassTagsRequest>,
    ) -> Result<Response<pb_cheatsheet_proto::Empty>, Status> {
        let req = req.into_inner();
        self.handle_remove_wm_class_tags(
            req.wm_class,
            req.tags
                .map(|t| t.tags.into_iter().collect())
                .unwrap_or_default(),
        )
        .await;
        Ok(Response::new(pb_cheatsheet_proto::Empty {}))
    }
}

pub async fn start_server(
    server: impl PbCheatsheetServerImpl,
    address: SocketAddr,
) -> anyhow::Result<()> {
    let service = PbCheatsheetServer::new(server);
    Server::builder()
        .add_service(service)
        .serve(address)
        .await
        .context("Serving Grpc server")
}

#[derive(Debug)]
pub struct PbCheatsheetClientStruct {
    client: PbCheatsheetClient<Channel>,
}

impl PbCheatsheetClientStruct {
    #[tracing::instrument(skip_all)]
    pub async fn new(address: &str) -> anyhow::Result<Self> {
        let client = PbCheatsheetClient::connect(format!("http://{address}"))
            .await
            .context("Connecting client to server")?;
        Ok(Self { client })
    }

    /// Report the focused window to the server
    #[tracing::instrument(skip_all)]
    pub async fn focused_window(&mut self, info: FocusedWindowInfo) -> anyhow::Result<()> {
        tracing::debug!("Focused window:\n{info:#?}");
        let req = Request::new(info.into());
        let _reply = self
            .client
            .focused_window(req)
            .await
            .context("Reporting focused window")?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_screen_info(&mut self) -> anyhow::Result<ScreenInfo> {
        tracing::debug!("Get screen info");
        let req = Request::new(pb_cheatsheet_proto::Empty {});
        let reply = self
            .client
            .get_screen_info(req)
            .await
            .context("Get screen info GPRC call")?;
        reply.into_inner().try_into()
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_cheatsheets_info(&mut self) -> anyhow::Result<CheatsheetsInfo> {
        tracing::debug!("Get cheatsheets info");
        let req = Request::new(pb_cheatsheet_proto::Empty {});
        let reply = self
            .client
            .get_cheatsheets_info(req)
            .await
            .context("Get cheatsheets info GPRC call")?;
        Ok(reply.into_inner().into())
    }

    #[tracing::instrument(skip_all)]
    pub async fn upload_cheatsheet_image(
        &mut self,
        image: CheatsheetImage,
        name: String,
        tags: HashSet<String>,
    ) -> anyhow::Result<()> {
        tracing::debug!("Uploading cheatsheet image\n{image:#?}");
        let req = Request::new(pb_cheatsheet_proto::UploadCheatsheetRequest {
            format: image.format.try_into()?,
            order: image.order.try_into()?,
            width: image.width,
            height: image.height,
            name,
            tags: Some(pb_cheatsheet_proto::Tags {
                tags: tags.into_iter().collect(),
            }),
            image_data: image.data,
        });
        let _reply = self
            .client
            .upload_cheatsheet(req)
            .await
            .context("Uploading cheatsheet image")?;
        tracing::debug!("Upload finished");
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove_cheatsheet(&mut self, name: String) -> anyhow::Result<()> {
        tracing::debug!("Remove cheatsheet");
        let req = Request::new(pb_cheatsheet_proto::RemoveCheatsheetRequest { name });
        let _reply = self
            .client
            .remove_cheatsheet(req)
            .await
            .context("Remove cheatsheet GRPC call")?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn upload_screenshot(
        &mut self,
        screenshot: CheatsheetImage,
        name: String,
    ) -> anyhow::Result<()> {
        tracing::debug!("Uploading screenshot\n{screenshot:#?}");
        let req = Request::new(pb_cheatsheet_proto::UploadScreenshotRequest {
            format: screenshot.format.try_into()?,
            order: screenshot.order.try_into()?,
            width: screenshot.width,
            height: screenshot.height,
            name,
            image_data: screenshot.data,
        });
        let _reply = self
            .client
            .upload_screenshot(req)
            .await
            .context("Uploading screenshot")?;
        tracing::debug!("Upload finished");
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn clear_screenshot(&mut self) -> anyhow::Result<()> {
        tracing::debug!("Clear screenshot");
        let req = Request::new(pb_cheatsheet_proto::Empty {});
        let _reply = self
            .client
            .clear_screenshot(req)
            .await
            .context("Clear screenshot GRPC call")?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn add_cheatsheet_tags(
        &mut self,
        name: String,
        tags: HashSet<String>,
    ) -> anyhow::Result<()> {
        tracing::debug!("Add cheatsheet tags");
        let req = Request::new(pb_cheatsheet_proto::AddCheatsheetTagsRequest {
            name,
            tags: Some(pb_cheatsheet_proto::Tags {
                tags: tags.into_iter().collect(),
            }),
        });
        let _reply = self
            .client
            .add_cheatsheet_tags(req)
            .await
            .context("Add cheatsheet tags GRPC call")?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove_cheatsheet_tags(
        &mut self,
        name: String,
        tags: HashSet<String>,
    ) -> anyhow::Result<()> {
        tracing::debug!("Remove cheatsheet tags");
        let req = Request::new(pb_cheatsheet_proto::RemoveCheatsheetTagsRequest {
            name,
            tags: Some(pb_cheatsheet_proto::Tags {
                tags: tags.into_iter().collect(),
            }),
        });
        let _reply = self
            .client
            .remove_cheatsheet_tags(req)
            .await
            .context("Remove cheatsheet tags GRPC call")?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn add_wm_class_tags(
        &mut self,
        wm_class: String,
        tags: HashSet<String>,
    ) -> anyhow::Result<()> {
        tracing::debug!("Add wm class tags");
        let req = Request::new(pb_cheatsheet_proto::AddWmClassTagsRequest {
            wm_class,
            tags: Some(pb_cheatsheet_proto::Tags {
                tags: tags.into_iter().collect(),
            }),
        });
        let _reply = self
            .client
            .add_wm_class_tags(req)
            .await
            .context("Add wm_class tags GRPC call")?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn remove_wm_class_tags(
        &mut self,
        wm_class: String,
        tags: HashSet<String>,
    ) -> anyhow::Result<()> {
        tracing::debug!("Remove wm class tags");
        let req = Request::new(pb_cheatsheet_proto::RemoveWmClassTagsRequest {
            wm_class,
            tags: Some(pb_cheatsheet_proto::Tags {
                tags: tags.into_iter().collect(),
            }),
        });
        let _reply = self
            .client
            .remove_wm_class_tags(req)
            .await
            .context("Remove wm_class tags GRPC call")?;
        Ok(())
    }
}
