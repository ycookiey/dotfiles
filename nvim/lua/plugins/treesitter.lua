return {
  "nvim-treesitter/nvim-treesitter",
  branch = "main",
  lazy = false,
  build = function()
    require("nvim-treesitter").update()
  end,
  config = function()
    local parsers = {
      "lua", "javascript", "typescript", "python", "json", "yaml", "html", "css",
      "markdown", "markdown_inline",
    }
    require("nvim-treesitter").install(parsers)

    vim.api.nvim_create_autocmd("FileType", {
      pattern = {
        "lua", "javascript", "typescript", "tsx", "python", "json", "yaml",
        "html", "css", "markdown",
      },
      callback = function(args)
        pcall(vim.treesitter.start)
        vim.bo[args.buf].indentexpr = "v:lua.require'nvim-treesitter'.indentexpr()"
      end,
    })
  end,
}
