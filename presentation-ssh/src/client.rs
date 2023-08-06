use std::io;

use noline::builder::EditorBuilder;
use playferrous_presentation::{
    bichannel::Bichannel,
    terminal::{PresentationToTerminalMsg, TerminalToPresentationMsg},
};
use thiserror::Error;
use tokio::{io::AsyncWriteExt, select};

use tracing::instrument;

use crate::{data_reader::DataReader, data_writer::DataWriter};

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
    mut presentation_connection: Bichannel<TerminalToPresentationMsg, PresentationToTerminalMsg>,
    mut data_reader: DataReader,
    mut data_writer: DataWriter,
) -> Result<(), ClientError> {
    let mut editor = EditorBuilder::new_unbounded()
        .with_unbounded_history()
        .build_async_tokio(&mut data_reader, &mut data_writer)
        .await?;
    while let Some(server_cmd) = loop {
        select! {
                line = editor
                .readline("> ", &mut data_reader, &mut data_writer) => {
                    let _ = presentation_connection
                    .s
                    .send(TerminalToPresentationMsg::ReadLine(line?.into()))
                    .await;
                }
                server_cmd = presentation_connection.r.recv() => break server_cmd
        }
    } {
        match server_cmd {
            PresentationToTerminalMsg::PrintLine(line)
            | PresentationToTerminalMsg::ErrorLine(line) => {
                data_writer
                    .write(line.replace("\n", "\r\n").as_bytes())
                    .await?;
                data_writer.flush().await?;
            }
        }
    }
    data_writer.shutdown().await?;
    Ok(())
}
