return {
  "iamcco/markdown-preview.nvim",
  cmd = { "MarkdownPreview", "MarkdownPreviewStop", "MarkdownPreviewToggle" },
  ft = { "markdown" },
  build = "cd app && npx --yes yarn install",
  init = function()
    vim.g.mkdp_auto_close = 0
    vim.g.mkdp_theme = "dark"
  end,
}
