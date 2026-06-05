-- Lua-filter: fordel tabelkolonner proportionalt så lange celler wrappes
function Table(el)
  local ncols = #el.colspecs
  if ncols == 0 then return el end
  -- Sæt kolonnebredder proportionalt (sum = 1.0)
  local width = 1.0 / ncols
  for i = 1, ncols do
    el.colspecs[i][2] = width
  end
  return el
end
