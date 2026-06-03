return {
  "nvim-flutter/flutter-tools.nvim",
  ft = "dart",
  init = function()
    -- pubspec.yaml 保存時に自動 pub get (VSCode runPubGetOnPubspecChanges 相当)
    vim.api.nvim_create_autocmd("BufWritePost", {
      group = vim.api.nvim_create_augroup("FlutterPubGetOnSave", { clear = true }),
      pattern = "pubspec.yaml",
      callback = function()
        require("lazy").load({ plugins = { "flutter-tools.nvim" } })
        vim.cmd("FlutterPubGet")
      end,
    })
  end,
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
