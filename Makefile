.PHONY: readme

readme: README.md

README.md: src/lib.rs
	@ cargo readme > README.md
	@ sed -i '' 's/\[\(`[^`]*`\)]/\1/g' README.md
