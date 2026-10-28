---@diagnostic disable: undefined-field, missing-fields
local fff = require('fff')
local rust = require('fff.rust')

for _, prompt in ipairs({ 'top', 'bottom' }) do
  describe('buffers picker with ' .. prompt .. ' prompt', function()
    local root, picker, buffers, original_buf

    before_each(function()
      original_buf = vim.api.nvim_get_current_buf()
      vim.o.columns = 140
      vim.o.lines = 32
      root = vim.fn.tempname()
      vim.fn.mkdir(root, 'p')
      root = vim.uv.fs_realpath(root)
      buffers = {}
      for _, name in ipairs({ 'alpha_beta.lua', 'bravo_gamma.lua', 'charlie_delta.lua', 'unlisted.lua', 'unloaded.lua' }) do
        local path = root .. '/' .. name
        vim.fn.writefile({ '-- preview: ' .. name, 'return true' }, path)
        local buf = vim.fn.bufadd(path)
        buffers[#buffers + 1] = buf
        vim.bo[buf].buflisted = name ~= 'unlisted.lua'
        if name ~= 'unloaded.lua' then vim.fn.bufload(buf) end
      end
      buffers[#buffers + 1] = vim.api.nvim_create_buf(true, false)
      vim.fn.writefile({ 'disk-only fixture' }, root .. '/disk_only.lua')
      vim.api.nvim_set_current_buf(buffers[1])
      vim.g.fff = {
        base_path = root,
        frecency = { enabled = false },
        history = { enabled = false },
        logging = { enabled = false },
      }
      rust.cleanup_file_picker()
      require('fff.core').mark_file_picker_uninitialized()
      require('fff.core').ensure_initialized()
      require('fff.file_picker').setup()
      rust.wait_for_initial_scan(10000)
      picker = require('fff.picker_ui.picker_ui')
      fff.buffers({ layout = { prompt_position = prompt } })
      assert.is_true(picker.state.active)
      assert.are.equal('buffers', picker.state.mode)
    end)

    after_each(function()
      if picker then picker.close() end
      vim.cmd('stopinsert')
      vim.wait(30, function() return false end)
      if vim.api.nvim_buf_is_valid(original_buf) then vim.api.nvim_set_current_buf(original_buf) end
      for _, buf in ipairs(buffers or {}) do
        if vim.api.nvim_buf_is_valid(buf) then vim.api.nvim_buf_delete(buf, { force = true }) end
      end
      rust.stop_background_monitor()
      rust.cleanup_file_picker()
      require('fff.core').mark_file_picker_uninitialized()
      vim.g.fff = nil
      vim.fn.delete(root, 'rf')
    end)

    it('lists only loaded, listed, named buffers and filters without suggestions', function()
      assert.are.equal(3, picker.state.pagination.total_matched)
      local names = vim.tbl_map(function(item) return item.name end, picker.state.filtered_items)
      table.sort(names)
      assert.are.same({ 'alpha_beta.lua', 'bravo_gamma.lua', 'charlie_delta.lua' }, names)

      picker.state.query = 'bravo_gamma'
      picker.update_results_sync()
      assert.are.equal(1, #picker.state.filtered_items)
      assert.are.equal('bravo_gamma.lua', picker.state.filtered_items[1].name)

      picker.state.query = 'disk_only'
      picker.update_results_sync()
      assert.are.equal(0, #picker.state.filtered_items)
      assert.is_nil(picker.state.suggestion_items)
      assert.is_nil(picker.state.suggestion_source)
    end)

    it('paginates, navigates, previews and selects buffers', function()
      vim.api.nvim_win_set_height(picker.state.list_win, 2)
      picker.update_results_sync()
      assert.are.equal(2, picker.state.pagination.page_size)
      assert.are.equal(2, #picker.state.filtered_items)
      local first_page = vim.deepcopy(picker.state.filtered_items)
      assert.is_true(picker.load_next_page())
      assert.are.equal(1, picker.state.pagination.page_index)
      assert.are.equal(1, #picker.state.filtered_items)
      assert.are.equal(3, picker.state.pagination.total_matched)
      assert.is_true(picker.load_previous_page())
      assert.are.same(first_page, picker.state.filtered_items)

      picker.state.cursor = 1
      if prompt == 'bottom' then
        picker.move_up()
      else
        picker.move_down()
      end
      assert.are.equal(2, picker.state.cursor)
      picker.update_preview()
      local selected = picker.state.filtered_items[picker.state.cursor]
      assert.is_true(vim.wait(2000, function()
        local lines = vim.api.nvim_buf_get_lines(picker.state.preview_buf, 0, -1, false)
        return lines[1] == '-- preview: ' .. selected.name
      end))
      picker.select('edit')
      assert.is_true(vim.wait(2000, function() return vim.api.nvim_buf_get_name(0) == selected.path end))
      assert.is_false(picker.state.active)
    end)
  end)
end
