return {
  "catppuccin/nvim",
  name = "catppuccin",
  priority = 1000,
  config = function()
    require("catppuccin").setup({
      flavour = "auto", -- vim.o.background に従い mocha(dark) / latte(light) を選択
      transparent_background = true,
    })

    local appearance_path = vim.fn.expand("~/.wezterm_appearance")

    local function apply_theme()
      local file = io.open(appearance_path, "r")
      local val = file and file:read("*l") or "dark"
      if file then file:close() end
      vim.o.background = (val == "light") and "light" or "dark"
      vim.cmd.colorscheme("catppuccin")
    end

    apply_theme()

    -- WezTermが書き出すファイルを監視してテーマ自動切替
    local watcher = vim.uv.new_fs_event()
    if watcher then
      watcher:start(appearance_path, {}, vim.schedule_wrap(function()
        apply_theme()
      end))
    end
  end,
}
