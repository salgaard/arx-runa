-- Lua-filter: tillad linjebrud ved '/' i kode-spans inde i tabeller.
-- Stier som src-tauri/src/tests/ er ét langt "ord" i LaTeX og wrappes ikke
-- medmindre vi eksplicit indsætter \allowbreak efter hvert slash.

local function escape_latex(s)
  -- Rækkefølge: backslash først
  s = s:gsub('\\', '\\textbackslash{}')
  s = s:gsub('%%', '\\%%')
  s = s:gsub('%$', '\\$')
  s = s:gsub('&',  '\\&')
  s = s:gsub('#',  '\\#')
  s = s:gsub('{',  '\\{')
  s = s:gsub('}',  '\\}')
  s = s:gsub('~',  '\\textasciitilde{}')
  s = s:gsub('%^', '\\textasciicircum{}')
  s = s:gsub('_',  '\\_')
  return s
end

function Table(el)
  return el:walk({
    Code = function(code)
      if not code.text:find('/') then return end
      local escaped = escape_latex(code.text)
      escaped = escaped:gsub('/', '/\\allowbreak{}')
      return pandoc.RawInline('latex', '\\texttt{' .. escaped .. '}')
    end
  })
end
