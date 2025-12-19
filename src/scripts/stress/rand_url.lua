-- 50% /api/v1/user
-- 25% /api/v1/client
-- 12.5% /api/v1/customer
-- 12.5% /api/v1/buyer

-- ===== Tunables (can be overridden via env) =====
local i_min   = tonumber(os.getenv("I_MIN") or "1")
local i_max   = tonumber(os.getenv("I_MAX") or "25000")

local language   = os.getenv("LANGUAGE") or "en"
local domain     = os.getenv("DOMAIN") or "advcache.example.com"
local user_id    = os.getenv("USER_ID") or "404"
local picked     = os.getenv("PICKED") or "helloworld"
local timezone   = os.getenv("TIMEZONE") or "UTC"

-- Weights for routing (defaults: 0.50 / 0.25 / 0.125 / 0.125)
local w_user      = tonumber(os.getenv("W_USER") or "0.5")
local w_client    = tonumber(os.getenv("W_CLIENT") or "0.25")
local w_customer  = tonumber(os.getenv("W_CUSTOMER") or "0.125")
local w_buyer     = tonumber(os.getenv("W_BUYER") or "0.125")

-- Common headers
local headers = {
  ["Accept-Encoding"]  = os.getenv("AE") or "gzip, deflate, br",
  ["Accept-Language"]  = os.getenv("ALANG") or "en-US,en;q=0.9"
}

-- ===== Per-thread seeding to avoid correlated RNG across threads =====
local thread_id = 0
function setup(thread)
  thread:set("tid", thread_id)
  thread_id = thread_id + 1
end

function init(args)
  local tid = tid or 0
  math.randomseed(os.time() + tid * 10007)
  for _ = 1, 5 do math.random() end
end

-- ===== Helpers =====
local function build_query(i)
  return
    "?user[id]=" .. user_id ..
    "&domain=" .. domain ..
    "&language=" .. language ..
    "&picked=" .. picked .. "_" .. i ..
    "&timezone=" .. timezone
end

-- precalc cumulative cutoffs to avoid float drift
local function normalize_weights()
  local sum = w_user + w_client + w_customer + w_buyer
  if sum <= 0 then
    -- fall back to defaults if someone passes zeros
    w_user, w_client, w_customer, w_buyer = 0.5, 0.25, 0.125, 0.125
    sum = 1.0
  end
  w_user     = w_user     / sum
  w_client   = w_client   / sum
  w_customer = w_customer / sum
  w_buyer    = w_buyer    / sum

  -- cumulative
  return {
    w_user,
    w_user + w_client,
    w_user + w_client + w_customer,
    1.0 -- implicit final
  }
end

local cut = normalize_weights()

-- ===== Request generator =====
request = function()
  local i = math.random(i_min, i_max)
  local q = build_query(i)

  local r = math.random()
  local path
  if r < cut[1] then
    path = "/api/v1/user" .. q
  elseif r < cut[2] then
    path = "/api/v1/client" .. q
  elseif r < cut[3] then
    path = "/api/v1/customer" .. q
  else
    path = "/api/v1/buyer" .. q
  end

  return wrk.format("GET", path, headers)
end
