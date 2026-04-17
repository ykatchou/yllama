use anyhow::Result;

use crate::llamacpp;

pub fn run() -> Result<()> {
    llamacpp::kill_server()
}
