local EQUIP_ID = 900001
local SOURCE_EQUIP_ID = 30023
local SHOP_GOOD_ID = 990001
local SOURCE_SHOP_GOOD_ID = 20013
local SSR_EQUIPMENT_SHOP_GOOD_BASE = 1800000
local UR_EQUIPMENT_SHOP_GOOD_BASE = 2800000
local EQUIPMENT_SHOP_PRICE = 200

local config_patched = false

local function assign_with_previous(previous, target, key, value)
  if type(previous) == "function" then
    previous(target, key, value)
  elseif type(previous) == "table" then
    previous[key] = value
  else
    rawset(target, key, value)
  end
end

local function deep_clone(value, seen)
  if type(value) ~= "table" then
    return value
  end
  seen = seen or {}
  if seen[value] ~= nil then
    return seen[value]
  end
  local result = {}
  seen[value] = result
  for key, child in pairs(value) do
    result[deep_clone(key, seen)] = deep_clone(child, seen)
  end
  return result
end

local function ensure_equipment(configs)
  if type(configs) ~= "table" or type(configs[SOURCE_EQUIP_ID]) ~= "table" then
    return configs
  end
  if type(configs[EQUIP_ID]) == "table" then
    return configs
  end

  local equipment = deep_clone(configs[SOURCE_EQUIP_ID])
  equipment.e_id = EQUIP_ID
  equipment.name = "未来試作砲"
  equipment.equip_prop = {{8, 90}, {3200, 300}}
  equipment.enhance_prop = {{8, 6}, {3200, 20}}
  equipment.drop_path = {}
  equipment.no_resolve = 1
  equipment.__blueoath_custom_equipment = true
  configs[EQUIP_ID] = equipment
  mod.info("injected equipment template id=" .. tostring(EQUIP_ID))
  return configs
end

local function ensure_shop_good(configs)
  if type(configs) ~= "table" or type(configs[SOURCE_SHOP_GOOD_ID]) ~= "table" then
    return configs
  end
  if type(configs[SHOP_GOOD_ID]) == "table" then
    return configs
  end

  local good = deep_clone(configs[SOURCE_SHOP_GOOD_ID])
  good.id = SHOP_GOOD_ID
  good.name = "未来試作砲×1"
  good.goods = {2, EQUIP_ID, 1}
  good.stock = 1
  good.manual_refresh_stock = 1
  good.goods_visible = 1
  good.__blueoath_custom_equipment = true
  configs[SHOP_GOOD_ID] = good
  mod.info("injected equipment shop good id=" .. tostring(SHOP_GOOD_ID))
  return configs
end

local function quality_equipment_shop_spec(good_id, equipment_configs)
  local numeric_good_id = tonumber(good_id)
  if numeric_good_id == nil or type(equipment_configs) ~= "table" then
    return nil
  end

  local good_base = nil
  local quality = nil
  local currency_id = nil
  if numeric_good_id > SSR_EQUIPMENT_SHOP_GOOD_BASE
      and numeric_good_id < UR_EQUIPMENT_SHOP_GOOD_BASE then
    good_base = SSR_EQUIPMENT_SHOP_GOOD_BASE
    quality = 4
    currency_id = 9
  elseif numeric_good_id > UR_EQUIPMENT_SHOP_GOOD_BASE then
    good_base = UR_EQUIPMENT_SHOP_GOOD_BASE
    quality = 5
    currency_id = 32
  else
    return nil
  end

  local equipment_id = numeric_good_id - good_base
  local equipment = equipment_configs[equipment_id]
  if type(equipment) ~= "table" or tonumber(equipment.quality) ~= quality then
    return nil
  end
  return equipment_id, currency_id, quality
end

local function ensure_quality_equipment_shop_good(configs, equipment_configs, requested_good_id)
  if type(configs) ~= "table" or type(equipment_configs) ~= "table" then
    return nil
  end

  local equipment_id, currency_id, quality = quality_equipment_shop_spec(
    requested_good_id,
    equipment_configs
  )
  if equipment_id == nil then
    return nil
  end

  local numeric_good_id = tonumber(requested_good_id)
  local existing = rawget(configs, numeric_good_id)
  if type(existing) == "table" then
    return existing
  end

  local source_id = quality == 4 and SOURCE_SHOP_GOOD_ID or 80002
  local source = rawget(configs, source_id)
  if type(source) ~= "table" then
    return nil
  end

  local good = deep_clone(source)
  good.id = numeric_good_id
  good.shelf_id = numeric_good_id
  good.name = (quality == 4 and "SSR Equipment" or "UR Equipment") .. " x1"
  good.goods = {2, equipment_id, 1}
  good.price = {{EQUIPMENT_SHOP_PRICE}}
  good.currency = {{5, currency_id}}
  good.price2 = {{5, currency_id, EQUIPMENT_SHOP_PRICE, EQUIPMENT_SHOP_PRICE, 100}}
  good.stock = -1
  good.manual_refresh_stock = 1
  good.goods_visible = 1
  good.__blueoath_generated_equipment_shop = true
  configs[numeric_good_id] = good
  return good
end

local function install_lazy_quality_equipment_shop_goods(configs, equipment_configs)
  if type(configs) ~= "table" or type(equipment_configs) ~= "table" then
    return
  end
  local current_metatable = getmetatable(configs)
  if current_metatable and current_metatable.__blueoath_lazy_equipment_shop then
    return
  end

  local metatable = {}
  if current_metatable ~= nil then
    for key, value in pairs(current_metatable) do
      metatable[key] = value
    end
  end
  local previous_index = current_metatable and current_metatable.__index or nil
  metatable.__index = function(target, key)
    local inherited = nil
    if type(previous_index) == "function" then
      inherited = previous_index(target, key)
    elseif type(previous_index) == "table" then
      inherited = previous_index[key]
    end
    if inherited ~= nil then
      return inherited
    end
    return ensure_quality_equipment_shop_good(target, equipment_configs, key)
  end
  metatable.__blueoath_lazy_equipment_shop = true
  setmetatable(configs, metatable)
end

local function patch_config_manager(manager)
  if config_patched or type(manager) ~= "table" then
    return
  end
  local original_get_data = manager.GetData
  local original_get_data_by_id = manager.GetDataById
  if type(original_get_data) ~= "function" or type(original_get_data_by_id) ~= "function" then
    error("configManager API is unavailable")
  end

  manager.GetData = function(name, ...)
    local data = original_get_data(name, ...)
    if name == "config_equip" then
      ensure_equipment(data)
    elseif name == "config_shop_goods" then
      ensure_shop_good(data)
      install_lazy_quality_equipment_shop_goods(data, manager.GetData("config_equip"))
    end
    return data
  end

  manager.GetDataById = function(name, id, ...)
    local numeric_id = tonumber(id)
    if name == "config_equip" and numeric_id == EQUIP_ID then
      local configs = manager.GetData("config_equip")
      return configs and configs[EQUIP_ID] or nil
    elseif name == "config_shop_goods" and numeric_id ~= nil then
      local configs = manager.GetData("config_shop_goods")
      if configs and type(configs[numeric_id]) == "table" then
        return configs[numeric_id]
      end
    end
    return original_get_data_by_id(name, id, ...)
  end

  config_patched = true
  mod.info("configManager equipment hooks installed")
end

local function safely_patch(manager)
  local ok, failure = xpcall(function()
    patch_config_manager(manager)
  end, debug.traceback)
  if not ok then
    mod.info("configManager equipment hook failed: " .. tostring(failure))
  end
end

local function install_when_config_manager_is_ready()
  local current = rawget(_G, "configManager")
  if current ~= nil then
    safely_patch(current)
    return
  end

  local meta = getmetatable(_G) or {}
  local previous_newindex = meta.__newindex
  meta.__newindex = function(target, key, value)
    assign_with_previous(previous_newindex, target, key, value)
    if key == "configManager" then
      meta.__newindex = previous_newindex
      safely_patch(value)
    end
  end
  setmetatable(_G, meta)
  mod.info("waiting for configManager")
end

function on_bootstrap()
  install_when_config_manager_is_ready()
end
