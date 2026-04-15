-- Luaモジュールキャッシュ（起動高速化、最優先で有効化）
if vim.loader then vim.loader.enable() end

-- シェル設定（Windows - PowerShell）
vim.opt.shell = "pwsh"
vim.opt.shellcmdflag = "-NoLogo -NoProfile -ExecutionPolicy RemoteSigned -Command"
vim.opt.shellquote = ""
vim.opt.shellxquote = ""
vim.opt.shellpipe = "| Out-File -Encoding UTF8 %s"
vim.opt.shellredir = "| Out-File -Encoding UTF8 %s"

-- 不要なビルトインプラグインを無効化
vim.g.loaded_netrw = 1
vim.g.loaded_netrwPlugin = 1
vim.g.loaded_matchit = 1 -- vim-matchup で代替
vim.g.loaded_zipPlugin = 1
vim.g.loaded_zip = 1
vim.g.loaded_gzip = 1
vim.g.loaded_tarPlugin = 1
vim.g.loaded_tar = 1
vim.g.loaded_tutor_mode_plugin = 1

-- leaderキー
vim.g.mapleader = " "

-- クリップボード同期
vim.opt.clipboard = "unnamedplus"

-- ターミナルタイトル（WezTerm連携に必要）
vim.opt.title = true

-- 相対行番号
vim.opt.number = true
vim.opt.relativenumber = true

-- 空白文字の可視化
vim.opt.list = true
vim.opt.listchars = {
  tab = "→ ",
  trail = "·",
  nbsp = "␣",
  extends = "»",
  precedes = "«",
}

-- 全角スペースハイライト
vim.api.nvim_create_autocmd("VimEnter", {
  callback = function()
    vim.api.nvim_set_hl(0, "ZenkakuSpace", { bg = "#4a3040" })
  end,
})
vim.api.nvim_create_autocmd({ "BufNewFile", "BufRead" }, {
  callback = function()
    vim.fn.matchadd("ZenkakuSpace", "　")
  end,
})

-- Markdownチェックボックスのトグル (Alt+L)
local function toggle_checkbox(line)
  if line:match("[%-%*%+] %[ %]") then
    return (line:gsub("([%-%*%+]) %[ %]", "%1 [x]", 1))
  elseif line:match("[%-%*%+] %[[xX]%]") then
    return (line:gsub("([%-%*%+]) %[[xX]%]", "%1 [ ]", 1))
  elseif line:match("^%s*[%-%*%+] ") then
    return (line:gsub("^(%s*[%-%*%+]) ", "%1 [ ] ", 1))
  end
  return nil
end

vim.keymap.set("n", "<A-l>", function()
  local result = toggle_checkbox(vim.api.nvim_get_current_line())
  if result then
    vim.api.nvim_set_current_line(result)
  else
    -- user-var経由でWezTermにペイン移動を要求（プロセス起動不要）
    vim.api.nvim_chan_send(2, "\x1b]1337;SetUserVar=pane_right=MQ==\x07")
  end
end, { desc = "Toggle markdown checkbox / pane right" })

vim.keymap.set("v", "<A-l>", function()
  local start_line = vim.fn.line("v")
  local end_line = vim.fn.line(".")
  if start_line > end_line then start_line, end_line = end_line, start_line end
  for lnum = start_line, end_line do
    local line = vim.fn.getline(lnum)
    local result = toggle_checkbox(line)
    if result then vim.fn.setline(lnum, result) end
  end
end, { desc = "Toggle markdown checkboxes (visual)" })

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
