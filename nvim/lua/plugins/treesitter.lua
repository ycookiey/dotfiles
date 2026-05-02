return {
  "nvim-treesitter/nvim-treesitter",
  branch = "master",
  event = { "BufReadPre", "BufNewFile" },
  build = ":TSUpdate",
  config = function()
    require("nvim-treesitter.configs").setup({
      ensure_installed = {
        "lua", "javascript", "typescript", "python", "json", "yaml", "html", "css",
        "markdown", "markdown_inline",
      },
      highlight = { enable = true },
      indent = { enable = true },
      matchup = { enable = true },
    })
  end,
}
