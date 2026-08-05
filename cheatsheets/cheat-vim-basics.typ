#import "./cheat-template.typ": cheat

#show: cheat.with(
  title: [Vim Cheatsheet Basics],
  icon: image("icons/vim.svg"),
  n_columns: 3
)

#table(
  table.header(table.cell(colspan: 2)[Horizontal Navigation]),
  [`0`], [Beginning of *line*],
  [`^`], [*First non-blank* character],
  [`B`], [Previous *WORD*],
  [`b`], [Previous *word*],
  [`h`], [Previous *character*],
  [`l`], [Next *character*],
  [`e`], [End of *word*],
  [`w`], [Beginning of next *word*],
  [`E`], [End of *WORD*],
  [`W`], [Beginning of next *WORD*],
  [`$`], [End of *line*],
)

#table(
  table.header(table.cell(colspan: 2)[Vertical Navigation]),
  [`gg`], [*first* line],
  [`^b`], [up *1 page*],
  [`^u`], [up *$1/2$ page*],
  [`^k`], [up *1 line*],
  [`^j`], [down *1 line*],
  [`^d`], [down *$1/2$ page*],
  [`^f`], [down *1 page*],
  [`^G`], [*last* line],
)

#table(
  table.header(table.cell(colspan: 2)[Insert Mode]),
  [`I`], [Enter insert mode at *beginning of line*],
  [`i`], [Enter insert mode *before cursor*],
  [`a`], [Enter insert mode *after cursor*],
  [`A`], [Enter insert mode at *end of line*],
  [`O`], [Enter insert mode *previous line*],
  [`o`], [Enter insert mode *next line*],
  [`esc`], [*Leave* insert mode],
  [`s`], [subsitute *character*],
  [`S`], [subsitute *line*],
  [`C`], [subsitute *from cursor* to end of line],
)

#table(
  table.header(table.cell(colspan: 2)[Visual Mode]),
  [`v`], [Enter visual mode to *select character* between a line],
  [`V`], [Enter visual mode to *select entire lines*],
  [`^v`], [Enter visual mode to *select boxes of text*],
  [`esc`], [*Leave* visual mode],
)

#table(
  table.header(table.cell(colspan: 2)[Operations]),
  table.cell(colspan: 2)[#h(1em)_Operations:_],
  [`d`], [Delete/cut],
  [`y`], [Yank/copy],
  [`c`], [Change],
  [`<`], [Shift left],
  table.cell(colspan: 2)[#h(1em)_Count:_],
  [`1`,`2`,#text(fill: red)[`n`]], [repeat #text(fill: red)[n] times],
  [`i`], [word at cursor position],
  [`i`], [WORD at cursor position],
  table.cell(colspan: 2)[#h(1em)_Motion:_],
  [`w`], [word],
  [`W`], [WORD],
  [`s`], [sentence],
  [`[,]`], [\[ \] block],
  [`(,)`], [\( \) block],
  [`{,}`], [\{ \} block],
  [`<,>`], [\< \> block],
  [`",'`], [quoted string],
)

#table(
  table.header(table.cell(colspan: 2)[Quick Operations]),
  [`p`], [Paste *after cursor*],
  [`P`], [Paste *before cursor*],
  [`u`], [Undo],
  [`^r`], [Redo],
  [`.`], [Repeat],
  [`.`], [Repeat],
  [`dd`], [Delete *current line*],
  [`yy`], [Yank *current line*],
  [`x`], [Delete character *after cursor*],
  [`%`], [Jump to  *matching parentheses*],
  [`r`], [Replace char *under cursor*],
  [`==`], [Auto-indent *current line*],
  [`<<`], [Shift *current line* left],
  [`>>`], [Shift *current line* right],
  [#text(fill: red)[`n`]`G`], [Jump to line #text(fill: red)[n]],
)

#table(
  table.header(table.cell(colspan: 2)[Commands]),
  [`:wq`], [Write and quit],
  [`:q!`], [Quit without saving],
  [`/`#text(fill: red)[`term`]], [Search for #text(fill: red)[term]],
  [`:s/`#text(fill: red)[`pattern`]`/`#text(fill: blue)[`replace`]`/g`], [Subsitute #text(fill: red)[pattern] with #text(fill: blue)[replace] in entire doc],
  [`:h `#text(fill: red)[`cmd`]], [Help for normal #text(fill: red)[cmd]],
  [`:! `#text(fill: red)[`cmd`]], [Execute external shell #text(fill: red)[cmd]],
)
