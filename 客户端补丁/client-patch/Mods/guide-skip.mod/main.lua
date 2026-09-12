-- Initial test client has no usable tutorial state. The stock guide manager can
-- reopen stage 10000/1200000 after login and leave copy-page input disabled.
-- Keep gameplay pages usable while server-side guide progress is repaired.

local patched = false
local mainline_patched = false

local function patch_guide_hub(guide_hub)
  if patched or type(guide_hub) ~= "table" then
    return guide_hub
  end

  local function always_can_login_to_main()
    return true
  end

  local function no_guide()
    return false
  end

  -- Do not run LOGIN_END trigger chain. It is what reopens stale tutorial
  -- stages before the server's GuideInfo is consumed by this client build.
  guide_hub.onLoginOK = function()
    return true
  end
  guide_hub.CanLoginToMain = always_can_login_to_main
  guide_hub.isInGuide = no_guide
  guide_hub.isInGuideBattle = no_guide
  guide_hub.isInFleetHeroGuide = no_guide
  guide_hub.enableElement = function() end
  guide_hub.disableElement = function() end
  guide_hub.__onReceiveGuideInfo = function() end

  patched = true
  mod.info("GuideHub login/locking hooks installed")
  return guide_hub
end

local function patch_open_mainline(module)
  if mainline_patched or type(module) ~= "table" then
    return module
  end

  local function disable_open_mainline(target)
    if type(target) ~= "table" then
      return
    end
    if type(target.doBehaviour) == "function" then
      target.doBehaviour = function(_, _, on_done)
        if type(on_done) == "function" then
          on_done()
        end
      end
      mainline_patched = true
    end
  end

  disable_open_mainline(module)
  local metatable = getmetatable(module)
  if type(metatable) == "table" and type(metatable.__index) == "table" then
    disable_open_mainline(metatable.__index)
  end
  if mainline_patched then
    mod.info("OpenMainLine guide behavior disabled")
  end
  return module
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
    elseif string.lower(tostring(name)) == "game.guide.guidebehaviours.openmainline" then
      patch_open_mainline(module)
    end
    return module
  end
  mod.info("waiting for game.guide.guidehub")
end

function on_bootstrap()
  install_require_hook()
end
