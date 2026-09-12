-- CopyPage reads this key from local Unity PlayerPrefs. Server prefs do not
-- populate that local store in this client build.
local KEY = "NewCopyButtomIndex"
local MIGRATED_KEY = "BlueRebirthCopyRouteMigratedV1"
local SEA_INDEX = 2

local prefs_patched = false
local prefs_method_watched = false
local seeded = false
local ui_patched = false
local copy_page_patched = false
local pve_room_page_patched = false
local pve_room_service_patched = false
local config_manager_patched = false
local copy_logic_patched = false
local pve_copy_detail_page_patched = false
local level_record_page_patched = false

local function assign_with_previous(previous, target, key, value)
  if type(previous) == "function" then
    previous(target, key, value)
  elseif type(previous) == "table" then
    previous[key] = value
  else
    rawset(target, key, value)
  end
end

local function numeric_field(value, ...)
  if type(value) ~= "table" then
    return nil
  end
  for _, key in ipairs({...}) do
    local number = tonumber(value[key])
    if number ~= nil then
      return number
    end
  end
  return nil
end

local function extract_copy_id(value, depth)
  if depth == nil then
    depth = 0
  end
  if depth > 3 then
    return nil
  end
  local direct = tonumber(value)
  if direct ~= nil and direct > 0 then
    return direct
  end
  if type(value) ~= "table" then
    return nil
  end
  for _, key in ipairs({"copyId", "copy_id", "nCopyId", "id"}) do
    local candidate = tonumber(value[key])
    if candidate ~= nil and candidate > 0 then
      return candidate
    end
  end
  for _, key in ipairs({"copyConfig", "copy_config", "data", "config"}) do
    local nested = extract_copy_id(value[key], depth + 1)
    if nested ~= nil then
      return nested
    end
  end
  for _, nested_value in pairs(value) do
    local nested = extract_copy_id(nested_value, depth + 1)
    if nested ~= nil then
      return nested
    end
  end
  return nil
end

local function find_display_config(manager, copy_id)
  local wanted = tonumber(copy_id)
  if wanted == nil or type(manager) ~= "table" then
    return nil, nil
  end

  local function lookup(id)
    local ok, rows = pcall(manager.GetData, "config_copy_display")
    if not ok or type(rows) ~= "table" then
      return nil, nil
    end
    for key, row in pairs(rows) do
      if type(row) == "table" and
          (tonumber(key) == id or numeric_field(row, "id") == id) then
        return row, id
      end
    end
    return nil, nil
  end

  local result, resolved_id = lookup(wanted)
  if result ~= nil then
    return result, resolved_id
  end

  -- Some client config rows use r_id as DBObject.id while the UI receives
  -- copy_id. Resolve that legacy alias before returning nil to the page.
  local ok, rows = pcall(manager.GetData, "config_copy")
  if ok and type(rows) == "table" then
    for _, row in pairs(rows) do
      if type(row) == "table" and numeric_field(row, "r_id", "id") == wanted then
        local display_id = numeric_field(row, "copy_id")
        if display_id ~= nil then
          result, resolved_id = lookup(display_id)
          if result ~= nil then
            return result, resolved_id
          end
        end
      end
    end
  end
  return nil, nil
end

local function patch_config_manager(manager)
  if config_manager_patched or type(manager) ~= "table" or
      type(manager.GetDataById) ~= "function" then
    return
  end
  local previous = manager.GetDataById
  manager.GetDataById = function(name, id, ...)
    local result = previous(name, id, ...)
    if name == "config_copy_display" then
      mod.info("config_copy_display lookup id=" .. tostring(id) ..
        " result=" .. tostring(type(result)))
      if result == nil and id ~= nil then
        local fallback, resolved_id = find_display_config(manager, id)
        if fallback ~= nil then
          mod.info("config_copy_display alias resolved id=" .. tostring(id) ..
            " -> " .. tostring(resolved_id))
          return fallback
        end
      end
    end
    return result
  end
  config_manager_patched = true
  mod.info("single-copy config lookup diagnostics installed")
end

local function patch_copy_logic(logic)
  if copy_logic_patched or type(logic) ~= "table" then
    return
  end
  local target = logic
  local metatable = getmetatable(target)
  if type(target.GetCopyDesConfig) ~= "function" and type(metatable) == "table" and
      type(metatable.__index) == "table" then
    target = metatable.__index
  end
  local previous = target.GetCopyDesConfig
  if type(previous) ~= "function" then
    return
  end
  target.GetCopyDesConfig = function(self, copy_id, ...)
    local result = previous(self, copy_id, ...)
    mod.info("GetCopyDesConfig copyId=" .. tostring(copy_id) ..
      " result=" .. tostring(type(result)))
    if result == nil then
      local fallback, resolved_id = find_display_config(rawget(_G, "configManager"), copy_id)
      if fallback ~= nil then
        mod.info("GetCopyDesConfig alias resolved id=" .. tostring(copy_id) ..
          " -> " .. tostring(resolved_id))
        return fallback
      end
    end
    return result
  end
  copy_logic_patched = true
  mod.info("single-copy display config fallback installed")
end

local function patch_pve_copy_detail_page(page)
  if pve_copy_detail_page_patched or type(page) ~= "table" then
    return
  end
  local target = page
  local metatable = getmetatable(target)
  if type(target.SingleBattleBtnClick) ~= "function" and type(metatable) == "table" and
      type(metatable.__index) == "table" then
    target = metatable.__index
  end
  local previous_single = target.SingleBattleBtnClick
  local previous_copy_click = target.CopyIdClick
  if type(previous_single) ~= "function" or type(previous_copy_click) ~= "function" then
    return
  end
  target.SingleBattleBtnClick = function(self, ...)
    local args = {...}
    if args[1] == nil then
      local fallback = extract_copy_id(self.m_singleCopy) or
        extract_copy_id(self.nCopyId) or extract_copy_id(self.m_copyId)
      if fallback ~= nil then
        args[1] = fallback
        mod.info("SingleBattleBtnClick filled copyId=" .. tostring(fallback))
      end
    end
    mod.info("SingleBattleBtnClick arg1=" .. tostring(args[1]) ..
      " singleCopy=" .. tostring(type(self.m_singleCopy)) ..
      " openCount=" .. tostring(self.m_openCount))
    return previous_single(self, table.unpack(args))
  end
  target.CopyIdClick = function(self, index, ...)
    if index == nil then
      index = extract_copy_id(self.m_singleCopy) or extract_copy_id(self.nCopyId)
      if index ~= nil then
        mod.info("CopyIdClick filled copyId=" .. tostring(index))
      end
    end
    mod.info("CopyIdClick index=" .. tostring(index) ..
      " singleCopy=" .. tostring(type(self.m_singleCopy)))
    return previous_copy_click(self, index, ...)
  end
  pve_copy_detail_page_patched = true
  mod.info("single-copy click diagnostics installed")
end

local function patch_level_record_page(page)
  if level_record_page_patched or type(page) ~= "table" then
    return
  end
  local target = page
  local metatable = getmetatable(target)
  if type(target.RegisterRecordToggle) ~= "function" and type(metatable) == "table" and
      type(metatable.__index) == "table" then
    target = metatable.__index
  end
  local previous = target.RegisterRecordToggle
  if type(previous) ~= "function" then
    return
  end
  target.RegisterRecordToggle = function(self, ...)
    local parent = rawget(self, "page")
    if type(parent) == "table" and parent.m_desConfInfo == nil then
      local copy_id = parent.nCopyId or parent.nCopyID or parent.m_copyId
      local fallback = find_display_config(rawget(_G, "configManager"), copy_id)
      if fallback == nil then
        fallback = find_display_config(rawget(_G, "configManager"), 1)
      end
      parent.m_desConfInfo = fallback or {
        evaluation_instructions = "",
        checkpoint_instructions = 0
      }
      mod.info("filled missing m_desConfInfo copyId=" .. tostring(copy_id))
    end
    return previous(self, ...)
  end
  level_record_page_patched = true
  mod.info("single-copy record config guard installed")
end

local function patch_player_prefs(player_prefs)
  if prefs_patched or type(player_prefs) ~= "table" then
    return
  end

  local original_get_int = player_prefs.GetInt
  local original_set_int = player_prefs.SetInt
  if type(original_get_int) ~= "function" or type(original_set_int) ~= "function" then
    if not prefs_method_watched then
      local metatable = getmetatable(player_prefs) or {}
      local previous_newindex = metatable.__newindex
      metatable.__newindex = function(target, key, value)
        assign_with_previous(previous_newindex, target, key, value)
        if key == "GetInt" or key == "SetInt" then
          patch_player_prefs(target)
        end
      end
      setmetatable(player_prefs, metatable)
      prefs_method_watched = true
      mod.info("waiting for PlayerPrefs methods")
    end
    return
  end

  local wrapped_get_int = function(uid, key, default_value)
    if not seeded and key == KEY then
      local migrated = 0
      local marker_ok = pcall(function()
        migrated = original_get_int(uid, MIGRATED_KEY, 0)
      end)
      if marker_ok and migrated ~= 1 then
        local ok = pcall(function()
          original_set_int(uid, KEY, SEA_INDEX)
          original_set_int(uid, MIGRATED_KEY, 1)
          if type(player_prefs.Save) == "function" then
            player_prefs.Save()
          end
        end)
        if ok then
          mod.info("copy route seeded for uid " .. tostring(uid) .. " to sea index " .. tostring(SEA_INDEX))
        else
          mod.info("failed to seed copy route")
        end
      else
        mod.info("copy route migration already complete")
      end
      seeded = true
    end
    return original_get_int(uid, key, default_value)
  end
  prefs_patched = true
  rawset(player_prefs, "GetInt", wrapped_get_int)

  mod.info("PlayerPrefs.GetInt hook installed")
end

local function patch_pve_room_page(page)
  if pve_room_page_patched or type(page) ~= "table" then
    return
  end
  local target = page
  local metatable = getmetatable(target)
  if type(target.CheckSpeedModeOpen) ~= "function" and
      type(metatable) == "table" and type(metatable.__index) == "table" then
    target = metatable.__index
  end
  local speed_check = target.CheckSpeedModeOpen
  local back_ready = target._BackReadySuc
  if type(speed_check) ~= "function" or type(back_ready) ~= "function" then
    return
  end
  target.CheckSpeedModeOpen = function(self, ...)
    local has_copy_data, copy_data = pcall(function()
      return Data.copyData
    end)
    if not has_copy_data or copy_data == nil then
      return false
    end
    local ok, result = pcall(speed_check, self, ...)
    if ok then
      return result
    end
    mod.info("PVERoomPage speed check fallback=false")
    return false
  end
  target._BackReadySuc = function(self, ...)
    local ok, room_data = pcall(function()
      return Data.pveRoomData:GetUserRoomInfo()
    end)
    if ok and type(room_data) == "table" and type(room_data.HeroList) == "table" and
        room_data.HeroList[1] ~= nil and room_data.HeroList[2] == nil then
      room_data.HeroList[2] = { HeroIdList = {}, HeroInfo = {}, StrategyId = 0 }
    end
    return back_ready(self, ...)
  end
  pve_room_page_patched = true
  mod.info("PVERoomPage guards installed")
end

local function patch_pve_room_service(service)
  if pve_room_service_patched or type(service) ~= "table" then
    return
  end
  local target = service
  local metatable = getmetatable(target)
  if type(target._DismissRoomRet) ~= "function" and
      type(metatable) == "table" and type(metatable.__index) == "table" then
    target = metatable.__index
  end
  local dismiss = target._DismissRoomRet
  if type(dismiss) ~= "function" then
    return
  end
  target._DismissRoomRet = function(self, ret, state, err, errmsg)
    local result = dismiss(self, ret, state, err, errmsg)
    if tonumber(err) == 0 then
      pcall(function() UIHelper.ClosePage("PVERoomPage") end)
    end
    return result
  end
  pve_room_service_patched = true
  mod.info("PveRoomService dismiss close hook installed")
end

local function patch_ui_helper(ui_helper)
  if ui_patched or type(ui_helper) ~= "table" then
    return
  end
  local original_open_page = ui_helper.OpenPage
  if type(original_open_page) ~= "function" then
    return
  end
  ui_helper.OpenPage = function(page_name, ...)
    local name = tostring(page_name)
    if name == "PVERoomPage" and type(_G.require) == "function" then
      local ok, page = pcall(_G.require, "ui.page.pve.pveroompage")
      if ok then
        patch_pve_room_page(page)
      end
    elseif name == "PVECopyDetailPage" and type(_G.require) == "function" then
      local ok, page = pcall(_G.require, "ui.page.pve.pvecopydetailpage")
      if ok then
        patch_pve_copy_detail_page(page)
      end
      local record_ok, record_page = pcall(_G.require, "ui.page.copy.levelrecordpartpage")
      if record_ok then
        patch_level_record_page(record_page)
      end
    end
    if string.find(name, "Copy", 1, true) or string.find(name, "Plot", 1, true) then
      mod.info("OpenPage " .. name)
    end
    return original_open_page(page_name, ...)
  end
  ui_patched = true
  mod.info("UIHelper.OpenPage diagnostics installed")
end

local function patch_copy_page(copy_page)
  if copy_page_patched or type(copy_page) ~= "table" then
    return
  end
  local target = copy_page
  local original_open = target.OpenTopPage
  local metatable = getmetatable(target)
  if type(original_open) ~= "function" and type(metatable) == "table" and type(metatable.__index) == "table" then
    target = metatable.__index
    original_open = target.OpenTopPage
  end
  if type(original_open) ~= "function" then
    return
  end
  target.OpenTopPage = function(self, ...)
    patch_player_prefs(rawget(_G, "PlayerPrefs"))
    return original_open(self, ...)
  end
  copy_page_patched = true
  mod.info("CopyPage.OpenTopPage hook installed")
end

local function install_require_hook()
  local original_require = _G.require
  if type(original_require) ~= "function" then
    return
  end
  _G.require = function(name, ...)
    local module = original_require(name, ...)
    if name == "ui.page.copy.copypage" then
      patch_copy_page(module)
    elseif name == "ui.page.pve.pvecopydetailpage" then
      patch_pve_copy_detail_page(module)
    elseif name == "ui.page.copy.levelrecordpartpage" then
      patch_level_record_page(module)
    elseif name == "ui.page.pve.pveroompage" then
      patch_pve_room_page(module)
    elseif name == "service.pveroomservice" then
      patch_pve_room_service(module)
    end
    return module
  end
end

function on_bootstrap()
  install_require_hook()
  mod.watch_global("configManager", patch_config_manager)
  mod.watch_global("Logic", function(logic)
    patch_config_manager(rawget(_G, "configManager"))
    if type(logic) == "table" then
      patch_copy_logic(rawget(logic, "copyLogic"))
    end
  end)
  mod.watch_global("PlayerPrefs", patch_player_prefs)
  -- PlayerPrefs object exists before its methods are attached.
  mod.watch_global("Logic", function()
    patch_player_prefs(rawget(_G, "PlayerPrefs"))
  end)
  mod.watch_global("Data", function()
    patch_player_prefs(rawget(_G, "PlayerPrefs"))
  end)
  mod.watch_global("UIHelper", patch_ui_helper)
end
