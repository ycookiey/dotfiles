return {
  "smoka7/multicursors.nvim",
  dependencies = { "nvimtools/hydra.nvim" },
  opts = {},
  cmd = { "MCstart", "MCvisual", "MCclear", "MCpattern", "MCvisualPattern", "MCunderCursor" },
  keys = {
    { "<Leader>m", "<cmd>MCstart<cr>", mode = { "n", "v" }, desc = "Multicursors" },
  },
}
