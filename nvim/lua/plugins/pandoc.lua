-- 注: g:pandoc#filetypes#pandoc_markdown / g:pandoc#filetypes#handled は
-- ftdetect の先行 source タイミング都合で init.lua 側で設定している。
return {
  "vim-pandoc/vim-pandoc",
  ft = { "pandoc" },
}
