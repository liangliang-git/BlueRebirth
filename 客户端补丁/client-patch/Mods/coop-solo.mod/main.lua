-- Enable single-player execution for the co-op room flow.
-- The stock client refuses room creation when the room-list filter is "all"
-- (copyId == 0), and requires match_player_num users before starting.

local SOLO_PLAYER_COUNT = 1

local room_logic_patched = false
local room_list_patched = false
local room_page_patched = false
local room_service_patched = false
local data_patched = false
local data_field_watcher_installed = false
local copy_data_patched = false
local room_data_patched = false

local function assign_with_previous(previous, target, key, value)
  if type(previous) == "function" then
    previous(target, key, value)
  elseif type(previous) == "table" then
    previous[key] = value
  else
    rawset(target, key, value)
  end
end

local function patch_room_logic(room_logic)
  if room_logic_patched or type(room_logic) ~= "table" then
    return
  end

  local previous = room_logic.GetRoomPlayerMax
  if type(previous) ~= "function" then
    error("PveRoomLogic.GetRoomPlayerMax is unavailable")
  end

  room_logic.GetRoomPlayerMax = function(self, copy_id, ...)
    previous(self, copy_id, ...)
    return SOLO_PLAYER_COUNT
  end

  room_logic_patched = true
  mod.info("PveRoomLogic single-player capacity hook installed")
end

local function patch_room_page(module)
  if room_page_patched or type(module) ~= "table" then
    return
  end

  local target = module
  local metatable = getmetatable(target)
  if type(target.CheckSpeedModeOpen) ~= "function" and
      type(metatable) == "table" and type(metatable.__index) == "table" then
    target = metatable.__index
  end

  local previous_speed_check = target.CheckSpeedModeOpen
  local previous_back_ready = target._BackReadySuc
  if type(previous_speed_check) ~= "function" or type(previous_back_ready) ~= "function" then
    return
  end

  target.CheckSpeedModeOpen = function(self, ...)
    local has_copy_data, copy_data = pcall(function()
      return Data.copyData
    end)
    if not has_copy_data or copy_data == nil then
      return false
    end
    local ok, result = pcall(previous_speed_check, self, ...)
    if ok then
      return result
    end
    mod.info("PVERoomPage speed check fallback=false: " .. tostring(result))
    return false
  end

  target._BackReadySuc = function(self, ...)
    local ok, room_data = pcall(function()
      return Data.pveRoomData:GetUserRoomInfo()
    end)
    if ok and type(room_data) == "table" and
        type(room_data.HeroList) == "table" and
        room_data.HeroList[1] ~= nil and room_data.HeroList[2] == nil then
      room_data.HeroList[2] = { HeroIdList = {}, HeroInfo = {}, StrategyId = 0 }
      mod.info("PVERoomPage auxiliary fleet fallback installed")
    end
    return previous_back_ready(self, ...)
  end

  room_page_patched = true
  mod.info("PVERoomPage missing-data guards installed")
end

local function patch_room_service(module)
  if room_service_patched or type(module) ~= "table" then
    return
  end

  local target = module
  local metatable = getmetatable(target)
  if type(target._DismissRoomRet) ~= "function" and
      type(metatable) == "table" and type(metatable.__index) == "table" then
    target = metatable.__index
  end

  local previous = target._DismissRoomRet
  if type(previous) ~= "function" then
    return
  end

  target._DismissRoomRet = function(self, ret, state, err, errmsg)
    local result = previous(self, ret, state, err, errmsg)
    if tonumber(err) == 0 then
      pcall(function()
        UIHelper.ClosePage("PVERoomPage")
      end)
      mod.info("PVERoomPage closed after room dismissal")
    end
    return result
  end

  room_service_patched = true
  mod.info("PveRoomService dismiss close hook installed")
end

local function patch_data(data)
  if type(data) ~= "table" then
    return
  end

  local patched_any = false
  local copy_data = rawget(data, "copyData")
  if not copy_data_patched and type(copy_data) == "table" and
      type(copy_data.GetCopyInfoById) == "function" then
    local previous = copy_data.GetCopyInfoById
    copy_data.GetCopyInfoById = function(self, copy_id, ...)
      local ok, result = pcall(previous, self, copy_id, ...)
      if ok and result ~= nil then
        return result
      end
      return { FirstPassTime = 0 }
    end
    copy_data_patched = true
    patched_any = true
  end

  local room_data = rawget(data, "pveRoomData")
  if not room_data_patched and type(room_data) == "table" and
      type(room_data.GetUserRoomInfo) == "function" then
    local previous = room_data.GetUserRoomInfo
    room_data.GetUserRoomInfo = function(self, ...)
      local result = previous(self, ...)
      if type(result) == "table" and type(result.HeroList) == "table" and
          result.HeroList[1] ~= nil and result.HeroList[2] == nil then
        result.HeroList[2] = { HeroIdList = {}, HeroInfo = {}, StrategyId = 0 }
      end
      return result
    end
    room_data_patched = true
    patched_any = true
  end

  if not data_field_watcher_installed then
    local metatable = getmetatable(data) or {}
    local previous_newindex = metatable.__newindex
    metatable.__newindex = function(target, key, value)
      assign_with_previous(previous_newindex, target, key, value)
      if key == "copyData" or key == "pveRoomData" then
        patch_data(target)
      end
    end
    local previous_index = metatable.__index
    local fallback_copy_data = {
      GetCopyInfoById = function()
        return { FirstPassTime = 0 }
      end
    }
    metatable.__index = function(target, key)
      if key == "copyData" and rawget(target, key) == nil then
        return fallback_copy_data
      end
      if type(previous_index) == "function" then
        return previous_index(target, key)
      elseif type(previous_index) == "table" then
        return previous_index[key]
      end
      return nil
    end
    setmetatable(data, metatable)
    data_field_watcher_installed = true
  end

  if patched_any then
    data_patched = true
    mod.info("co-op data fallbacks installed")
  end
end

local function watch_service(service)
  if type(service) ~= "table" then
    return
  end

  local current = rawget(service, "pveRoomService")
  if current ~= nil then
    patch_room_service(current)
    return
  end

  local metatable = getmetatable(service) or {}
  local previous_newindex = metatable.__newindex
  metatable.__newindex = function(target, key, value)
    assign_with_previous(previous_newindex, target, key, value)
    if key == "pveRoomService" then
      patch_room_service(value)
    end
  end
  setmetatable(service, metatable)
  mod.info("waiting for Service.pveRoomService")
end

local function patch_page_from_require()
  if room_page_patched or type(_G.require) ~= "function" then
    return
  end
  local ok, module = pcall(_G.require, "ui.page.pve.pveroompage")
  if ok then
    patch_room_page(module)
  else
    mod.info("PVERoomPage require deferred: " .. tostring(module))
  end
end

local function patch_service_from_require()
  if room_service_patched or type(_G.require) ~= "function" then
    return
  end
  local ok, module = pcall(_G.require, "service.pveroomservice")
  if ok then
    patch_room_service(module)
  else
    mod.info("PveRoomService require deferred: " .. tostring(module))
  end
end

local function patch_ui_helper(ui_helper)
  if type(ui_helper) ~= "table" or type(ui_helper.OpenPage) ~= "function" then
    return
  end
  local previous = ui_helper.OpenPage
  ui_helper.OpenPage = function(page_name, ...)
    if tostring(page_name) == "PVERoomPage" then
      patch_page_from_require()
    end
    return previous(page_name, ...)
  end
  mod.info("PVERoomPage lazy patch installed")
end

local function watch_room_logic(logic)
  if type(logic) ~= "table" then
    return
  end

  local current = rawget(logic, "pveRoomLogic")
  if current ~= nil then
    patch_room_logic(current)
    return
  end

  local metatable = getmetatable(logic) or {}
  local previous_newindex = metatable.__newindex
  metatable.__newindex = function(target, key, value)
    assign_with_previous(previous_newindex, target, key, value)
    if key == "pveRoomLogic" then
      patch_room_logic(value)
    end
  end
  setmetatable(logic, metatable)
  mod.info("waiting for Logic.pveRoomLogic")
end

local function choose_default_copy(self)
  if tonumber(self.copyId) ~= 0 then
    return true
  end

  local logic = rawget(_G, "Logic")
  local room_logic = type(logic) == "table" and rawget(logic, "pveRoomLogic") or nil
  if type(room_logic) ~= "table" or type(room_logic.GetAllCopyList) ~= "function" then
    mod.info("cannot create solo room: PveRoomLogic.GetAllCopyList unavailable")
    return false
  end

  local copy_list = room_logic:GetAllCopyList()
  if type(copy_list) ~= "table" then
    mod.info("cannot create solo room: copy list unavailable")
    return false
  end

  for _, copy_id in ipairs(copy_list) do
    if tonumber(copy_id) ~= nil and tonumber(copy_id) > 0 then
      self.copyId = tonumber(copy_id)
      mod.info("all-copy solo room redirected to copyId=" .. tostring(self.copyId))
      return true
    end
  end

  mod.info("cannot create solo room: copy list is empty")
  return false
end

local function patch_room_list(module)
  if room_list_patched or type(module) ~= "table" then
    return
  end

  local target = module
  local metatable = getmetatable(target)
  if type(target._ClickTrue) ~= "function" and
      type(metatable) == "table" and type(metatable.__index) == "table" then
    target = metatable.__index
  end

  local previous = target._ClickTrue
  if type(previous) ~= "function" then
    return
  end

  target._ClickTrue = function(self, ...)
    if not choose_default_copy(self) then
      return previous(self, ...)
    end
    return previous(self, ...)
  end

  room_list_patched = true
  mod.info("PveRoomListPage all-copy create hook installed")
end

local function install_require_hook()
  local previous_require = _G.require
  if type(previous_require) ~= "function" then
    error("require is unavailable")
  end

  _G.require = function(name, ...)
    local module = previous_require(name, ...)
    local module_name = string.lower(tostring(name))
    if string.find(module_name, "pveroomlogic", 1, true) then
      patch_room_logic(module)
    elseif string.find(module_name, "pveroomlistpage", 1, true) then
      patch_room_list(module)
    elseif string.find(module_name, "pveroompage", 1, true) then
      patch_room_page(module)
    elseif string.find(module_name, "pveroomservice", 1, true) then
      patch_room_service(module)
    end
    return module
  end
  mod.info("waiting for PveRoomListPage")
end

function on_bootstrap()
  install_require_hook()
  mod.watch_global("Logic", watch_room_logic)
  mod.watch_global("Service", watch_service)
  mod.watch_global("UIHelper", patch_ui_helper)
  mod.watch_global("Data", patch_data)
  patch_page_from_require()
  patch_service_from_require()
end
