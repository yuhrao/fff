---@diagnostic disable: undefined-field, missing-fields
-- Config test for the opt-in `show_hidden` setting: on a non-git root,
-- include dotfiles and files under hidden directories while still
-- respecting ignore rules and always excluding `.git/` internals. Git-repo
-- behavior must stay unchanged regardless of this setting.

local fff_rust = require('fff.rust')

local function relative_paths(result)
  local paths = {}
  for _, item in ipairs(result.items) do
    table.insert(paths, (item.relative_path:gsub('\\', '/')))
  end
  return paths
end

local function contains(list, value)
  for _, v in ipairs(list) do
    if v == value then return true end
  end
  return false
end

--- Index `root` with the given init opts and return all relative paths.
local function index_and_list(root, opts)
  fff_rust.init_file_picker(root, opts)
  fff_rust.wait_for_initial_scan(15000)
  local result = fff_rust.fuzzy_search_files('', 1, nil, 100, 3, 0, 1000)
  local paths = relative_paths(result)
  pcall(fff_rust.stop_background_monitor)
  pcall(fff_rust.cleanup_file_picker)
  return paths
end

describe('show_hidden', function()
  it('non-git root: default (false) excludes dotfiles and hidden dirs', function()
    local root = vim.fn.tempname()
    vim.fn.mkdir(root .. '/.config', 'p')
    local fd = assert(io.open(root .. '/.env', 'w'))
    fd:write('SECRET=1\n')
    fd:close()
    fd = assert(io.open(root .. '/.config/settings.json', 'w'))
    fd:write('{}\n')
    fd:close()
    fd = assert(io.open(root .. '/index.js', 'w'))
    fd:write('x\n')
    fd:close()

    local paths = index_and_list(root, { show_hidden = false })
    vim.fn.delete(root, 'rf')

    assert.is_true(contains(paths, 'index.js'))
    assert.is_false(contains(paths, '.env'))
    assert.is_false(contains(paths, '.config/settings.json'))
  end)

  it('non-git root: true includes dotfiles and hidden dirs', function()
    local root = vim.fn.tempname()
    vim.fn.mkdir(root .. '/.config', 'p')
    local fd = assert(io.open(root .. '/.env', 'w'))
    fd:write('SECRET=1\n')
    fd:close()
    fd = assert(io.open(root .. '/.config/settings.json', 'w'))
    fd:write('{}\n')
    fd:close()
    fd = assert(io.open(root .. '/index.js', 'w'))
    fd:write('x\n')
    fd:close()

    local paths = index_and_list(root, { show_hidden = true })
    vim.fn.delete(root, 'rf')

    assert.is_true(contains(paths, 'index.js'))
    assert.is_true(contains(paths, '.env'))
    assert.is_true(contains(paths, '.config/settings.json'))
  end)

  it('non-git root: true still respects a .ignore file', function()
    local root = vim.fn.tempname()
    vim.fn.mkdir(root, 'p')
    local fd = assert(io.open(root .. '/.ignore', 'w'))
    fd:write('.env.ignored\n')
    fd:close()
    fd = assert(io.open(root .. '/.env.ignored', 'w'))
    fd:write('SECRET=1\n')
    fd:close()
    fd = assert(io.open(root .. '/.env.local', 'w'))
    fd:write('OTHER=1\n')
    fd:close()

    local paths = index_and_list(root, { show_hidden = true })
    vim.fn.delete(root, 'rf')

    assert.is_true(contains(paths, '.env.local'))
    assert.is_false(contains(paths, '.env.ignored'))
  end)

  it('non-git root: true never surfaces .git internals', function()
    local root = vim.fn.tempname()
    vim.fn.mkdir(root .. '/.git', 'p')
    local fd = assert(io.open(root .. '/.git/config', 'w'))
    fd:write('[core]\n')
    fd:close()
    fd = assert(io.open(root .. '/index.js', 'w'))
    fd:write('x\n')
    fd:close()

    local paths = index_and_list(root, { show_hidden = true })
    vim.fn.delete(root, 'rf')

    assert.is_true(contains(paths, 'index.js'))
    for _, p in ipairs(paths) do
      assert.is_nil(p:find('%.git/'), '.git internals must never be indexed: ' .. p)
    end
  end)

  it('git root: show_hidden does not change results', function()
    local root = vim.fn.tempname()
    vim.fn.mkdir(root, 'p')
    assert(os.execute('git init -q ' .. vim.fn.shellescape(root)))
    local fd = assert(io.open(root .. '/.env', 'w'))
    fd:write('SECRET=1\n')
    fd:close()
    fd = assert(io.open(root .. '/Cargo.toml', 'w'))
    fd:write('x\n')
    fd:close()

    local off_paths = index_and_list(root, { show_hidden = false })
    local on_paths = index_and_list(root, { show_hidden = true })
    vim.fn.delete(root, 'rf')

    table.sort(off_paths)
    table.sort(on_paths)
    assert.are.same(off_paths, on_paths)
    assert.is_true(contains(off_paths, '.env'), 'git roots already show hidden files today')
  end)
end)
