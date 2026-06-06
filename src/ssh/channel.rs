//! SSH channel abstraction for PTY I/O.
//!
//! Wraps russh Channel for ergonomic read/write with timeout support.

use anyhow::Result;
use russh::ChannelMsg;
use tokio::time::{timeout, Duration};

/// Read from the SSH channel with a timeout.
pub async fn read_with_timeout(
    channel: &mut russh::Channel<russh::client::Msg>,
    timeout_dur: Duration,
) -> Result<Option<Vec<u8>>> {
    match timeout(timeout_dur, channel.wait()).await {
        Ok(Some(ChannelMsg::Data { data })) => Ok(Some(data.to_vec())),
        Ok(Some(ChannelMsg::Eof)) | Ok(None) => Ok(None),
        Ok(_) => Ok(None),
        Err(_) => Ok(None), // Timeout
    }
}
