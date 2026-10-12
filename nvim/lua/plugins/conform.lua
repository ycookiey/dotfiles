return {
  "stevearc/conform.nvim",
  ft = "dart",
  config = function()
    require("conform").setup({
      formatters_by_ft = {
        -- "prefer": flutter-tools が起動する dartls の formatting(中身は dart format と同じ dart_style)を使い、
        -- 保存のたびに dart CLI(Dart VM)を起動しない。dartls が attach していなければ dart_format(CLI)に戻る。
        -- format_on_save ではなく filetype 側に置くと、:ConformInfo にも LSP の経路が出る
        -- TODO: Windows では exepath("dart") が Flutter SDK の拡張子なし sh スクリプトを返し、CLI fallback は
        -- vim.system の ENOENT で失敗する。直し方(dart-sdk の dart.exe を名指し / dart.bat と長い timeout / 現状維持)は未決
        dart = { "dart_format", lsp_format = "prefer" },
      },
      format_on_save = function(bufnr)
        if vim.bo[bufnr].filetype ~= "dart" then return end
        return { timeout_ms = 3000 }
      end,
    })
  end,
}
