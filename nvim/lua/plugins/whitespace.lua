return {
  {
    "echasnovski/mini.trailspace",
    event = { "BufReadPost", "BufNewFile" },
    opts = {
      only_in_normal_buffers = true,
    },
    keys = {
      { "<leader>tw", function() MiniTrailspace.trim() end, desc = "Trim trailing whitespace" },
      { "<leader>tl", function() MiniTrailspace.trim_last_lines() end, desc = "Trim trailing empty lines" },
    },
  },
}
