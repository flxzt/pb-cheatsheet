#import "./cheat-template.typ": cheat

#show: cheat.with(
  title: [Bash Cheatsheet Basics],
  icon: image("icons/bash.svg"),
  n_columns: 3
)

#table(
  table.header[Navigation],
  [`^a`], [Jump to *start of line*],
  [`^e`], [Jump to *end of line*],
  [`^f`], [Move *character forward*],
  [`^b`], [Move *character back*],
  [`<Alt> f`], [Move *word forward*],
  [`<Alt> b`], [Move *word back*],
)

#table(
  table.header[Shortcuts],
  [`^u`], [Delete everything *before* the cursor],
  [`^k`], [Delete everything *at and after* the cursor],
  [`^w`], [Delete *word* before the cursor],
  [`^l`], [*Clear* screen],
  [`^z`], [*Stop* current running command],
  [`^p`], [*Previous* command in history],
  [`^p`], [*Next* command in history],
)

#table(
  table.header(table.cell(colspan: 2)[Redirection, Pipes]),
  [`<`], [Feed *stdin* with *file contents*],
  [`>`], [write *stdout* to file, *overwriting*],
  [`>>`], [write *stdout* to file, *appending*],
  [`2>`], [write *stderr* to file, *overwriting*],
  [`2>>`], [write *stderr* to file, *appending*],
  [`2>&1`], [redirect *stderr* to *stdout*],
  [`2> /dev/null`], [discard *stderr*],
  [`|`], [Connect *stdout* to *stdin* of two commands],
)

#table(
  table.header[Operations],
  [`!!`], [*Expand* last *command*],
  [`!-`#text(fill: red)[`n`]], [*Expand* #text(fill: red)[n]'th most recent *command*],
  [`!`#text(fill: red)[`cmd`]], [*Expand* most recent invocation of #text(fill: red)[cmd]],
  [`!^`], [*Repeat* first *argument* of last command],
  [`!$`], [*Repeat* last *argument* of last command],
  [`!*`], [*Repeat* all *argument* of last command],
)

#table(
  table.header(table.cell(colspan: 2)[Variables, Substitution]),
  [`VAR=VAL`], [Assign *VAL* as environmnet variable *VAR*],
  [`VAR=VAL`#text(fill: red)[`cmd`]], [Run #text(fill: red)[cmd] with assigned env var *VAR*],
  [`export VAR`], [Make variable available in subprocesses],
  [`${VAR}`], [Subsitute with value assigned to variable *VAR*],
  [`${VAR:-DEFAULT}`], [Subsitute with value assigned to variable *VAR*, subsitute with *DEFAULT* if not set],
  [`${VAR:=DEFAULT}`], [Subsitute with value assigned to variable *VAR*, set to *DEFAULT* if not set],
  [`${VAR:+VAL}`], [Subsitute with *VAL* if *VAR* is set (it's value does not get used)],
  [`$(`#text(fill: red)[`cmd`]`)`], [Evaluate and substitute with output of #text(fill: red)[cmd]],
  [`[[ `#text(fill: red)[`expr`]`]]`], [Brackets subsitute with `test` command with given #text(fill: red)[expr]],
  [`[[ ! ` #text(fill: red)[`expr`]`]]`], [*Not* #text(fill: red)[expr]],
  [`[[ -e `#text(fill: red)[`path`]`]]`], [Test if #text(fill: red)[path] *exists*],
  [`[[ -d `#text(fill: red)[`path`]`]]`], [Test if #text(fill: red)[path] is *directory*],
  [`[[ -f `#text(fill: red)[`path`]`]]`], [Test if #text(fill: red)[path] is *file*],
)

#table(
  table.header[Options],
  [`set -e`], [Exit immediately when a command returns with *non-zero* code (opt-out with `cmd || true`)],
  [`set -u`], [Exit immediately when a undefined variables gets used],
  [`set -x`], [Prints each line before executing],
  [`set -o pipefail`], [Code gets propagated in a pipeline when one commands returns *non-zero*],
  [`set -euxo pipefail`], ["Strict" mode combining previous modes, recommended in scripts],
)
