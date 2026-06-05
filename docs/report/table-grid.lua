-- table-grid.lua
-- Renders all tables as LaTeX longtable with full cell borders (grid style).

local stringify = pandoc.utils.stringify

local esc_map = {
  ['\\'] = '\\textbackslash{}',
  ['#']  = '\\#',
  ['$']  = '\\$',
  ['%']  = '\\%',
  ['&']  = '\\&',
  ['_']  = '\\_',
  ['{']  = '\\{',
  ['}']  = '\\}',
  ['~']  = '\\textasciitilde{}',
  ['^']  = '\\textasciicircum{}',
}

local function esc(s)
  s = s:gsub('\\', esc_map['\\'])
  return (s:gsub('[#$%%&_{}~^]', esc_map))
end

local function cell_tex(cell)
  return esc(stringify(cell.contents))
end

function Table(el)
  local ncols = #el.colspecs
  if ncols == 0 then return el end

  local colspec = '|'
  for i = 1, ncols do
    local w = el.colspecs[i][2] or (1.0 / ncols)
    colspec = colspec .. string.format('p{%.4f\\linewidth}|', w * 0.92)
  end

  local lines = {
    '\\begin{longtable}{' .. colspec .. '}',
    '\\hline',
  }

  if el.head and el.head.rows then
    for _, row in ipairs(el.head.rows) do
      local cells = {}
      for _, cell in ipairs(row.cells) do
        table.insert(cells, '{\\bfseries ' .. cell_tex(cell) .. '}')
      end
      table.insert(lines, table.concat(cells, ' & ') .. ' \\\\[2pt]')
      table.insert(lines, '\\hline\\hline')
    end
  end
  table.insert(lines, '\\endhead')

  for _, body in ipairs(el.bodies) do
    for _, row in ipairs(body.body) do
      local cells = {}
      for _, cell in ipairs(row.cells) do
        table.insert(cells, cell_tex(cell))
      end
      table.insert(lines, table.concat(cells, ' & ') .. ' \\\\')
      table.insert(lines, '\\hline')
    end
  end

  table.insert(lines, '\\end{longtable}')
  return pandoc.RawBlock('latex', table.concat(lines, '\n'))
end
