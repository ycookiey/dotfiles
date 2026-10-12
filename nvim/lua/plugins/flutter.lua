return {
  "nvim-flutter/flutter-tools.nvim",
  ft = "dart",
  -- pubspec.yaml を開いた時点でも読み込む。保存時の pub get (VSCode runPubGetOnPubspecChanges 相当) は
  -- flutter-tools が setup で登録する BufWritePost */pubspec.yaml に任せる (自前で足すと二重に走る)
  event = { "BufReadPre pubspec.yaml", "BufNewFile pubspec.yaml" },
  dependencies = {
    "nvim-lua/plenary.nvim",
    "nvim-telescope/telescope.nvim",
    "neovim/nvim-lspconfig",
    "hrsh7th/cmp-nvim-lsp",
  },
  config = function()
    require("flutter-tools").setup({
      ui = { border = "rounded", notification_style = "native" },
      decorations = {
        statusline = { app_version = false, device = true, project_config = false },
      },
      widget_guides = { enabled = true },
      closing_tags = { highlight = "Comment", prefix = "// ", enabled = true },
      dev_log = { enabled = true, open_cmd = "tabedit" },
      lsp = {
        capabilities = require("cmp_nvim_lsp").default_capabilities(),
        color = { enabled = true, background = false, foreground = false, virtual_text = true },
        settings = {
          showTodos = true,
          completeFunctionCalls = true,
          renameFilesWithClasses = "prompt",
          enableSnippets = true,
          updateImportsOnRename = true,
        },
      },
    })

    -- Flutter コマンドショートカット
    local map = vim.keymap.set
    map("n", "<leader>fr", "<cmd>FlutterRun<cr>", { desc = "Flutter run" })
    map("n", "<leader>fq", "<cmd>FlutterQuit<cr>", { desc = "Flutter quit" })
    map("n", "<leader>fR", "<cmd>FlutterRestart<cr>", { desc = "Flutter restart" })
    map("n", "<leader>fl", "<cmd>FlutterReload<cr>", { desc = "Flutter reload (hot)" })
    map("n", "<leader>fd", "<cmd>FlutterDevices<cr>", { desc = "Flutter devices" })
    map("n", "<leader>fe", "<cmd>FlutterEmulators<cr>", { desc = "Flutter emulators" })
    map("n", "<leader>fo", "<cmd>FlutterOutlineToggle<cr>", { desc = "Flutter outline" })
    map("n", "<leader>fL", "<cmd>FlutterLogClear<cr>", { desc = "Flutter log clear" })
  end,
}
