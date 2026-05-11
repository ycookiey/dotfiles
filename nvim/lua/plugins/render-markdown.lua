return {
  "MeanderingProgrammer/render-markdown.nvim",
  ft = { "markdown", "Avante" },
  dependencies = {
    "nvim-treesitter/nvim-treesitter",
    "nvim-tree/nvim-web-devicons",
  },
  opts = {
    render_modes = { "n", "c", "t", "v", "V", "\22" },
    anti_conceal = { enabled = false },
  },
}
