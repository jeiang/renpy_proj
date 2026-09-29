# sed patches applied to the extracted Rift sources (cwd = game/). One line per manual fix. (sources use CRLF)
sed -i '' 's/^\( *\)import __builtin__/\1import builtins as __builtin__/' utils/debug/logger.rpy
