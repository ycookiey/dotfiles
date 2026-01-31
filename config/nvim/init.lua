-- leaderキー
vim.g.mapleader = " "

-- クリップボード同期
vim.opt.clipboard = "unnamedplus"

-- 相対行番号
vim.opt.number = true
vim.opt.relativenumber = true

-- lazy.nvim ブートストラップ
local lazypath = vim.fn.stdpath("data") .. "/lazy/lazy.nvim"
if not vim.uv.fs_stat(lazypath) then
  vim.fn.system({
    "git", "clone", "--filter=blob:none",
    "https://github.com/folke/lazy.nvim.git",
    "--branch=stable",
    lazypath,
  })
end
vim.opt.rtp:prepend(lazypath)

require("lazy").setup("plugins")
