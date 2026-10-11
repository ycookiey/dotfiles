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

    -- "light" / "dark" を返す。ファイルがない・空(WezTerm が "w" で truncate した直後)なら nil
    local function read_appearance()
      local file = io.open(appearance_path, "r")
      if not file then return nil end
      local val = vim.trim(file:read("*l") or "")
      file:close()
      if val == "" then return nil end
      return (val == "light") and "light" or "dark"
    end

    local function apply_theme(bg)
      -- 値が変わらず catppuccin のままなら何もしない(hi clear と全 highlight の再定義を避ける)
      local is_catppuccin = (vim.g.colors_name or ""):find("^catppuccin") ~= nil
      if is_catppuccin and vim.o.background == bg then return end
      -- colors_name がある状態で background を変えると、Nvim がここで colorscheme を読み直し、
      -- catppuccin は background に合う flavour を選ぶ。既に合っていれば再適用しない
      vim.o.background = bg
      if vim.g.colors_name ~= "catppuccin-" .. (bg == "light" and "latte" or "mocha") then
        vim.cmd.colorscheme("catppuccin")
      end
    end

    apply_theme(read_appearance() or "dark")

    -- WezTermが書き出すファイルを監視してテーマ自動切替
    -- WezTerm は config を評価するたびに無条件で書くため、連続イベントを 100ms debounce する
    local watcher = vim.uv.new_fs_event()
    local timer = vim.uv.new_timer()
    if watcher and timer then
      local on_change = vim.schedule_wrap(function()
        local bg = read_appearance()
        if bg then apply_theme(bg) end
      end)
      local ok, err = watcher:start(appearance_path, {}, function(watch_err)
        if watch_err then return end
        timer:stop()
        timer:start(100, 0, on_change)
      end)
      if not ok then
        watcher:close()
        timer:close()
        -- ファイルがない環境(WezTerm 未使用)では監視できないのが正常なので黙る
        if vim.uv.fs_stat(appearance_path) then
          vim.notify("colorscheme: cannot watch " .. appearance_path .. ": " .. tostring(err), vim.log.levels.WARN)
        end
      end
    else
      if watcher then watcher:close() end
      if timer then timer:close() end
    end
  end,
}
