//! Zed dev extension that launches `tc-lsp` for TransCompilator English.

use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct TcEngExtension;

impl zed::Extension for TcEngExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let command = worktree.which("tc-lsp").ok_or_else(|| {
            "tc-lsp was not found on PATH. Run editors/zed/install_zed_config.sh first."
                .to_string()
        })?;
        Ok(Command {
            command,
            args: vec!["--stdio".to_string()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(TcEngExtension);
