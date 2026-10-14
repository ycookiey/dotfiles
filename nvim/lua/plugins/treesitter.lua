return {
  "nvim-treesitter/nvim-treesitter",
  branch = "main",
  lazy = false,
  build = function()
    require("nvim-treesitter").update()
  end,
  config = function()
    -- 使う parser の一覧。install と FileType の対象はここから導出する
    -- dart は入れない: flutter-tools (dartls) の semantic tokens と色付けが重なるため
    local parsers = {
      "lua", "javascript", "typescript", "tsx", "python", "json", "yaml", "html", "css",
      "markdown", "markdown_inline",
    }

    -- install() はすべて入っていても parser 定義の再読込と install 済みの走査をするため、足りない分だけ渡す
    local ts = require("nvim-treesitter")
    local installed = ts.get_installed("parsers")
    local missing = vim.tbl_filter(function(lang)
      return not vim.list_contains(installed, lang)
    end, parsers)
    if #missing > 0 then
      ts.install(missing)
    end

    vim.api.nvim_create_autocmd("FileType", {
      -- parser 名と filetype は一致しないことがある (tsx → typescriptreact 等) ため、登録済みの対応から導出する
      pattern = vim.iter(parsers):map(vim.treesitter.language.get_filetypes):flatten():totable(),
      callback = function(args)
        if pcall(vim.treesitter.start, args.buf) then
          vim.bo[args.buf].indentexpr = "v:lua.require'nvim-treesitter'.indentexpr()"
        end
      end,
    })
  end,
}
