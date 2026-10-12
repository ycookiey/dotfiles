return {
  "nvim-telescope/telescope.nvim",
  dependencies = {
    "nvim-lua/plenary.nvim",
    {
      "nvim-telescope/telescope-fzf-native.nvim",
      -- README の CMake 手順。Windows では MSVC Build Tools が要る (make/gcc は不要)。build/libfzf.dll を生成
      build = "cmake -S. -Bbuild -DCMAKE_BUILD_TYPE=Release && cmake --build build --config Release --target install",
    },
  },
  keys = {
    { "<leader>ff", "<cmd>Telescope find_files<cr>" },
    { "<leader>fg", "<cmd>Telescope live_grep<cr>" },
    { "<leader>fb", "<cmd>Telescope buffers<cr>" },
    { "<leader>fh", "<cmd>Telescope help_tags<cr>" },
  },
  config = function()
    require("telescope").setup({
      defaults = {
        sorting_strategy = "ascending",
        layout_config = { prompt_position = "top" },
      },
    })
    -- libfzf.dll が未ビルド (Build Tools なし等) でも Telescope 本体は既定の sorter で使えるようにする
    local ok, err = pcall(require("telescope").load_extension, "fzf")
    if not ok then
      vim.notify(
        "telescope-fzf-native を読み込めない (build/libfzf.dll 未生成?)。既定の sorter で動作する\n" .. tostring(err),
        vim.log.levels.WARN
      )
    end
  end,
}
