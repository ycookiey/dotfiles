return {
  "stevearc/conform.nvim",
  ft = "dart",
  config = function()
    -- Flutter SDK の bin/dart は拡張子の無い sh スクリプトと dart.bat。Windows の exepath("dart") は、
    -- 'shell' が sh 系なら sh スクリプトを返して dart_format (CLI) の起動が vim.system の ENOENT で失敗し、
    -- pwsh 等なら dart.bat を返して動くが、1 回 2 秒台かかる (SDK の更新確認を挟む。dart.exe は 1 秒弱)。
    -- flutter (無ければ dart) のある bin から SDK 同梱の <bin>/cache/dart-sdk/bin/dart.exe を導いて名指しする。
    -- 見つからなければ既定の "dart" のまま
    local function sdk_dart_exe()
      for _, name in ipairs({ "flutter", "dart" }) do
        local exe = vim.fn.exepath(name)
        if exe ~= "" then
          local dart = vim.fs.joinpath(vim.fs.dirname(exe), "cache", "dart-sdk", "bin", "dart.exe")
          if vim.fn.executable(dart) == 1 then return dart end
        end
      end
    end
    local dart_exe = sdk_dart_exe()
    require("conform").setup({
      formatters_by_ft = {
        -- "prefer": flutter-tools が起動する dartls の formatting(中身は dart format と同じ dart_style)を使い、
        -- 保存のたびに dart CLI(Dart VM)を起動しない。dartls が attach していなければ dart_format(CLI)に戻る。
        -- format_on_save ではなく filetype 側に置くと、:ConformInfo にも LSP の経路が出る
        dart = { "dart_format", lsp_format = "prefer" },
      },
      -- 既定の dart_format の定義 (args 等) を引き継ぎ、command だけ差し替える
      formatters = dart_exe and { dart_format = { command = dart_exe } } or nil,
      format_on_save = function(bufnr)
        if vim.bo[bufnr].filetype ~= "dart" then return end
        return { timeout_ms = 3000 }
      end,
    })
  end,
}
