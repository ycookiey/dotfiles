-- 注: .md を pandoc filetype にさせない g:pandoc#filetypes#pandoc_markdown = 0 は、
-- ftdetect が lazy.nvim の起動時に先行 source されるため init.lua 側で設定している。
return {
  "vim-pandoc/vim-pandoc",
  ft = { "pandoc" },
}
