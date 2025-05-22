use compact_str::CompactString;
use crux_core::capability::Operation;
use serde::{Deserialize, Serialize};

/// An operation to send a file to the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadOperation {
    pub mime_type: Option<CompactString>,
    pub file_name: Option<CompactString>,
    pub content: Vec<u8>,
}

/// An empty response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FileDownloadResponse {}

impl Operation for FileDownloadOperation {
    type Output = FileDownloadResponse;
}
