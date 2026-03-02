local ENGLISH = "1033"
local JAPANESE = "1041"
local CMD = "im-select.exe"

local ime_off = CMD .. " " .. ENGLISH
local ime_on = CMD .. " " .. JAPANESE

local group = vim.api.nvim_create_augroup("ime-control", { clear = true })

-- InsertLeave / CmdlineLeave: IME → English
vim.api.nvim_create_autocmd({ "InsertLeave", "CmdlineLeave" }, {
  group = group,
  callback = function()
    vim.fn.system(ime_off)
  end,
})

-- VimLeave / FocusLost: IME → Japanese（nvim外で困らないように）
vim.api.nvim_create_autocmd({ "VimLeave", "FocusLost" }, {
  group = group,
  callback = function()
    vim.fn.system(ime_on)
  end,
})

-- ESC ESC: IME → English + clear search highlights
-- 単体ESCはフローティングウィンドウ等のネイティブ動作を維持
vim.keymap.set("n", "<Esc><Esc>", function()
  vim.fn.system(ime_off)
  vim.cmd.nohlsearch()
end, { desc = "IME → English / Clear highlights" })

return {}
