return {
  "stevearc/conform.nvim",
  ft = "dart",
  config = function()
    require("conform").setup({
      formatters_by_ft = {
        dart = { "dart_format" },
      },
      format_on_save = function(bufnr)
        if vim.bo[bufnr].filetype ~= "dart" then return end
        return { timeout_ms = 3000, lsp_format = "fallback" }
      end,
    })
  end,
}
