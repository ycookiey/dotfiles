use super::fd_fzf::fd_fzf_pick;
use crate::protocol::ShellAction;

/// fd でカレント配下のディレクトリを列挙 → fzf で選択 → cd
/// zoxide hook (pre_prompt) が cd 後に自動で履歴へ add する
pub fn run(args: &[String]) {
    if let Some(selected) = fd_fzf_pick("d", "zf> ", args) {
        let action = ShellAction {
            cd: Some(selected),
            ..Default::default()
        };
        action.print();
    }
}
