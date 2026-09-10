-- Initial test client has no usable tutorial state. The stock guide manager can
-- reopen stage 10000/1200000 after login and leave copy-page input disabled.
-- Keep gameplay pages usable while server-side guide progress is repaired.

local patched = false
local config_patched = false

local function install_tower_config_diagnostics(config_manager)
  if config_patched or type(config_manager) ~= "table" then
    return
  end
  local original = config_manager.GetDataById
  if type(original) ~= "function" then
    return
  end

  config_manager.GetDataById = function(name, id, ...)
    local value = original(name, id, ...)
    if name == "config_chapter" or name == "config_chapter_tower" or name == "config_tower_topic" then
      if id == 30001 or id == 40001 or id == 100001 then
        if type(value) == "table" then
          mod.info("tower config " .. tostring(name) .. "[" .. tostring(id) .. "] found")
          if name == "config_chapter" then
            mod.info("tower chapter relation=" .. tostring(value.relation_chapter_id) .. " next=" .. tostring(value.next_chapter))
          elseif name == "config_chapter_tower" then
            mod.info("tower definition level=" .. tostring(value.level) .. " topics=" .. tostring(value.tower_topic and #value.tower_topic or 0))
          elseif name == "config_tower_topic" then
            mod.info("tower topic copies=" .. tostring(value.copy_list and #value.copy_list or 0))
          end
        else
          mod.info("tower config " .. tostring(name) .. "[" .. tostring(id) .. "] missing")
        end
      end
    end
    return value
  end
  config_patched = true
  mod.info("tower config diagnostics installed")
end

local function patch_guide_methods(target, skip_login, no_guide)
  if type(target) ~= "table" then
    return
  end

  -- The module in this build exposes methods through either its table or its
  -- __index table. Patch both forms; otherwise an instance can bypass hooks.
  target.onLogin = skip_login
  target.OnLogin = skip_login
  target.LoginOk = skip_login
  target.onLoginOK = skip_login
  target.SetGuide = function() end
  target.CanLoginToMain = function()
    return true
  end
  target.isInGuide = no_guide
  target.IsInGuide = no_guide
  target.isInGuideBattle = no_guide
  target.IsInGuideBattle = no_guide
  target.isInFleetHeroGuide = no_guide
  target.IsInFleetHeroGuide = no_guide
  target.enableElement = function() end
  target.disableElement = function() end
  target.__onReceiveGuideInfo = function() end
end

local function patch_guide_hub(guide_hub)
  if patched or type(guide_hub) ~= "table" then
    return guide_hub
  end

  local function no_guide()
    return false
  end

  -- These names are from this client build. `LoginOk` is case-sensitive;
  -- the previous onLoginOK hook never ran, so LOGIN_END reopened stage 10000.
  local skip_login = function()
    return true
  end
  patch_guide_methods(guide_hub, skip_login, no_guide)
  local metatable = getmetatable(guide_hub)
  if type(metatable) == "table" and type(metatable.__index) == "table" then
    patch_guide_methods(metatable.__index, skip_login, no_guide)
  end

  patched = true
  mod.info("GuideHub login/locking hooks installed")
  return guide_hub
end

local function install_require_hook()
  local original_require = _G.require
  if type(original_require) ~= "function" then
    error("require is unavailable")
  end

  _G.require = function(name, ...)
    local module = original_require(name, ...)
    if name == "game.guide.guidehub" then
      patch_guide_hub(module)
    end
    return module
  end
  mod.info("waiting for game.guide.guidehub")
end

function on_bootstrap()
  install_require_hook()
  mod.watch_global("configManager", install_tower_config_diagnostics)
end
