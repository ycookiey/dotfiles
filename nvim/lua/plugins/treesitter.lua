return {
  "nvim-treesitter/nvim-treesitter",
  event = { "BufReadPre", "BufNewFile" },
  build = ":TSUpdate",
  config = function()
    require("nvim-treesitter").setup({
      ensure_installed = { "lua", "javascript", "typescript", "python", "json", "yaml", "html", "css" },
      highlight = { enable = true },
      indent = { enable = true },
      matchup = { enable = true },
    })
  end,
}
