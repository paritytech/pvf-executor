use wasmparser::BinaryReaderError;
use std::error::Error;

#[derive(Debug)]
pub enum PvfError {
	FilesystemError(std::io::Error),
	ParseError(BinaryReaderError),
	ExportNotFound,
}

impl From<BinaryReaderError> for PvfError {
	fn from(err: BinaryReaderError) -> Self {
		PvfError::ParseError(err)
	}
}

impl std::fmt::Display for PvfError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "PVF Error: {:?}", self)
	}
}

impl Error for PvfError {}
