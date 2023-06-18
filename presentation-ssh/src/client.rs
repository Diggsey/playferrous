use std::io;

use noline::builder::EditorBuilder;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    select,
};

use playferrous_presentation::{TerminalClientCommand, TerminalConnection, TerminalServerCommand};
use tracing::{debug, instrument};

use crate::{data_reader::DataReader, data_writer::DataWriter, null_buf::NullBuf};

type NolineError = noline::error::Error<io::Error, io::Error>;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Noline error {0:?}")]
    Noline(NolineError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<NolineError> for ClientError {
    fn from(value: NolineError) -> Self {
        Self::Noline(value)
    }
}

#[instrument(level = "debug", skip_all)]
pub async fn run(
    mut terminal_connection: TerminalConnection,
    mut data_reader: DataReader,
    mut data_writer: DataWriter,
) -> Result<(), ClientError> {
    let mut null_buf = NullBuf::new();
    let mut editor = EditorBuilder::new_unbounded()
        .with_unbounded_history()
        .build_async_tokio(&mut data_reader, &mut data_writer)
        .await?;
    while let Some(server_cmd) = loop {
        select! {
                _ = data_reader.read_buf(&mut null_buf) => {}
                server_cmd = terminal_connection.receiver.recv() => break server_cmd
        }
    } {
        debug!(server_cmd = debug(&server_cmd));
        match server_cmd {
            TerminalServerCommand::RequestLine { prompt } => {
                let line = editor
                    .readline(&prompt, &mut data_reader, &mut data_writer)
                    .await?;
                let _ = terminal_connection
                    .sender
                    .send(TerminalClientCommand::Line(line.into()))
                    .await;
            }
            TerminalServerCommand::Print { text } => {
                data_writer
                    .write(text.replace("\n", "\r\n").as_bytes())
                    .await?;
            }
        }
    }
    data_writer.shutdown().await?;
    Ok(())
}
