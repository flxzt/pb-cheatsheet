#let cheat(
  title: none,
  icon: none,
  n_columns: 2,
  doc,
) = {
  set page(
    width: 1024pt,
    height: 758pt,
    margin: 6pt,
  )
  set text(
    font: "Bookerly",
    size: 15pt
  )
  show raw: it => box(
    stroke: rgb("#dddddd") + 0.5pt,
    inset: 2pt,
    radius: 1pt,
    text(
      font: "Inconsolata",
      weight: "bold",
      size: 15pt,
      it
    )
  )

  set par(
    leading: 4pt
  )

  let table_stroke(color) = (x, y) => (
    left: none, right: none, top: none, bottom: if y == 0 { color } else { 0pt },
  )

  let table_fill(color) = (x, y) => {
    if calc.odd(y) {
      rgb(color)
    } else { none }
  }

  set table(
    align: left + horizon, columns: (2fr, 3fr),
    fill: table_fill(rgb("F2F2F2")), stroke: table_stroke(rgb("21222C")),
  )
  set table.header(repeat: false)

  show table.cell.where(y: 0): set text(weight: "bold")
  show table.cell.where(y: 0): it => {
    table.cell(colspan: 2)[#it]
  }

  columns(n_columns, gutter: 6pt)[
    #align(center)[
      #box(height: 20pt)[
        #if icon != none {
          set image(height: 100%)
          box(icon, baseline: 25%)
        }
        #text(20pt, title)
      ]
    ]
    #doc
  ]
}