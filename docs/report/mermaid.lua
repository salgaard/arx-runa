-- Lua-filter: render mermaid code blocks to PNG via mmdc
local counter = 0

function CodeBlock(el)
  local is_mermaid = false
  for _, cls in ipairs(el.classes) do
    if cls == "mermaid" then is_mermaid = true; break end
  end
  if not is_mermaid then return nil end

  counter = counter + 1
  io.stderr:write("mermaid.lua: rendering block #" .. counter .. "\n")

  local tmpdir = os.getenv("TEMP") or os.getenv("TMP") or "C:\\Temp"
  local base   = tmpdir .. "\\mermaid_" .. counter
  local infile = base .. ".mmd"
  local outfile = base .. ".png"

  local f = assert(io.open(infile, "w"))
  f:write(el.text)
  f:close()

  local cmd = '"C:\\Users\\chris\\AppData\\Roaming\\npm\\mmdc.cmd" -i "' .. infile .. '" -o "' .. outfile .. '" -w 1400 -b white'
  io.stderr:write("mermaid.lua: running: " .. cmd .. "\n")
  local ret = os.execute(cmd)
  io.stderr:write("mermaid.lua: exit code: " .. tostring(ret) .. "\n")

  local img = io.open(outfile, "r")
  if img then
    img:close()
    io.stderr:write("mermaid.lua: OK -> " .. outfile .. "\n")
    local caption = el.attributes["caption"] or ""
    return pandoc.Para({ pandoc.Image({pandoc.Str(caption)}, outfile, caption) })
  end

  io.stderr:write("mermaid.lua: FEJL - ingen PNG genereret\n")
  return nil
end
