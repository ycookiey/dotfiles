return {
  "folke/noice.nvim",
  event = "VeryLazy",
  dependencies = {
    "MunifTanjim/nui.nvim",
    {
      "rcarriga/nvim-notify",
      opts = {
        -- transparent_background で Normal に bg がないため、現在の catppuccin flavour の base を
        -- 描画のたびに取る(dark=mocha / light=latte)
        background_colour = function()
          local ok, palettes = pcall(require, "catppuccin.palettes")
          return ok and palettes.get_palette().base or "#1e1e2e"
        end,
      },
    },
  },
  opts = {
    lsp = {
      override = {
        ["vim.lsp.util.convert_input_to_markdown_lines"] = true,
        ["vim.lsp.util.stylize_markdown"] = true,
        ["cmp.entry.get_documentation"] = true,
      },
    },
    presets = {
      bottom_search = true,
      command_palette = true,
      long_message_to_split = true,
      inc_rename = false,
      lsp_doc_border = false,
    },
  },
}
