return {
  "mikavilpas/yazi.nvim",
  dependencies = { "nvim-lua/plenary.nvim" },
  keys = {
    { "<leader>e", "<cmd>Yazi<cr>", desc = "Yazi (current file)" },
    { "<leader>E", "<cmd>Yazi cwd<cr>", desc = "Yazi (cwd)" },
  },
}
