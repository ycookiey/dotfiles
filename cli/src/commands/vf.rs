use super::fd_fzf::fd_fzf_pick;
use std::process::Command;

/// fd でカレント配下のファイルを列挙 → fzf で選択 → nvim で開く
/// 追加 args は fd にそのまま渡る（例: `vf .rs` で絞り込み）
pub fn run(args: &[String]) {
    if let Some(selected) = fd_fzf_pick("f", "vf> ", args) {
        let _ = Command::new("nvim").arg(&selected).status();
    }
}
