use compact_str::CompactString;
use crux_core::capability::{CapabilityContext, Operation};
use serde::{Deserialize, Serialize};

/// A request to send a file to the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadRequest {
    pub mime_type: Option<CompactString>,
    pub file_name: Option<CompactString>,
    pub content: Vec<u8>,
}

/// An empty response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FileDownloadResponse {}

impl Operation for FileDownloadRequest {
    type Output = FileDownloadResponse;
}

/// The FileDownload capability API.
///
/// This capability allows the app to request the user to download a file.
pub struct FileDownload<Ev> {
    context: CapabilityContext<FileDownloadRequest, Ev>,
}

impl<Ev> Clone for FileDownload<Ev> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
        }
    }
}

impl<Ev> crux_core::Capability<Ev> for FileDownload<Ev> {
    type Operation = FileDownloadRequest;
    type MappedSelf<MappedEv> = FileDownload<MappedEv>;

    fn map_event<F, NewEv>(&self, f: F) -> Self::MappedSelf<NewEv>
    where
        F: Fn(NewEv) -> Ev + Send + Sync + 'static,
        Ev: 'static,
        NewEv: 'static + Send,
    {
        FileDownload::new(self.context.map_event(f))
    }

    #[cfg(feature = "typegen")]
    fn register_types(
        generator: &mut crux_core::typegen::TypeGen,
    ) -> crux_core::typegen::FileDownloadResult {
        generator.register_type::<Self::Operation>()?;
        generator.register_type::<<Self::Operation as Operation>::Output>()?;
        Ok(())
    }
}

impl<Ev> FileDownload<Ev>
where
    Ev: 'static,
{
    pub fn new(context: CapabilityContext<FileDownloadRequest, Ev>) -> Self {
        Self { context }
    }

    pub fn file_download(
        &self,
        content: Vec<u8>,
        file_name: Option<impl Into<CompactString>>,
        mime_type: Option<impl Into<CompactString>>,
    ) {
        let req = FileDownloadRequest {
            content,
            file_name: file_name.map(Into::into),
            mime_type: mime_type.map(Into::into),
        };
        self.context.spawn({
            let this = self.clone();
            async move {
                this.file_download_async(req).await;
            }
        });
    }

    pub async fn file_download_async(&self, req: FileDownloadRequest) {
        self.context.notify_shell(req).await
    }
}
