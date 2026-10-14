# Frozen Material Room v1 producer

`r6_content.rs` and `r6_material-room.json` are verbatim source/asset copies from
engine commit `a2b3eebbaadbd93889c807903c6d3151272344d1`, the R6 client retained at
the close of R10. Keep them unchanged when editing the current parser or scene.

`../data_contract.rs` compares the historical producer with the current producer
through the same compositor, all three client cameras, four stages, serial/eight
workers, moving/static ticks and history reset. This isolates producer and client
camera regressions. It does not independently freeze the compositor; the workspace
already carries historical renderer regression tests.

The oracle retains its original unit tests. Their executions are additional test
coverage, not separate feature claims. Integration tests depend only on the
`ascii-material-room` and `ascii-render` crates, never the laboratory actor/world.
