use crux_core::capability::Operation;
use ecow::EcoString;
use serde::{Deserialize, Serialize};

/// An operation to send a file to the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadOperation {
    pub mime_type: Option<EcoString>,
    pub file_name: Option<EcoString>,
    pub content: Vec<u8>,
}

/// An empty response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FileDownloadResponse {}

impl Operation for FileDownloadOperation {
    type Output = FileDownloadResponse;
}
