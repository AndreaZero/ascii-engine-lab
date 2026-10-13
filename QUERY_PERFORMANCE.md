# R4 — query per ombre e costo della rappresentazione

Consegna del 13 ottobre 2026. Il laboratorio distingue ora raggi primari, riflessi e ombre; le ombre della fixture hanno un percorso booleano con uscita anticipata. Il vantaggio misurato è modesto e dipende dalla visuale. **L'immagine resta identica**: 24/24 catture GPU coincidono con R3. Il nuovo eseguibile è usato dal launcher abituale.

## Intervento e confini

`Scene::occluded(ray, time, stage, max_distance)` risponde alla domanda necessaria per l'ombra, invece di richiedere sempre tutti i dati del punto più vicino. Il default riproduce esattamente la precedente query `trace(..., false)` e il filtro distanza/materiale: i client esistenti continuano a funzionare. `shade` conserva direzione, offset, soglia `dist - 0.09` e condizione sulla luce.

`WorldSnapshot` specializza la risposta: interroga l'attore come prima, determina la distanza della lampada e visita suolo, muratura e vegetazione fermandosi al primo ostacolo sufficiente. Non serve costruire un nuovo `Hit` ambiente completo per restituire il booleano. Le query primarie e riflesse, i materiali, la griglia, gli intersettori matematici e l'animazione sono invariati. Il fallback per riuso dello snapshot a un altro tempo/stage conserva il contratto precedente.

La semantica già visibile richiede attenzione: un emissivo più vicino maschera gli ostacoli successivi. Il glow dell'attore viene visitato prima dell'ambiente e vince una parità di distanza; la lampada ambiente viene visitata dopo e la perde. I limiti restano distinti (`<` per il primo, `<=` per la seconda). Non è una nuova simulazione fisica delle ombre o un sistema generale di trasparenza: è una specializzazione equivalente del comportamento R3.

Manteniamo la specializzazione circoscritta e il default di riferimento, con regressioni automatiche. I dati non giustificano una riscrittura generale in compute GPU, SIMD o una gerarchia spaziale in questa tranche. Non abbiamo aggiunto un editor, gameplay o modifiche alla qualità artistica.

## Prima misura: dove vanno le query

`examples/query_profile.rs` raccoglie le chiamate in una composizione seriale non temporizzata. Poi riproduce i gruppi di raggi contro lo stesso snapshot, con un cronometro per batch, senza lock o allocazioni per raggio nella parte misurata. Tre warm-up e 21 batch; ordine dei gruppi ruotato. Cloth, 240×135 celle, tempo 1 s, tre pose camera standard.

| Camera | Raggi primari | Raggi riflessi | Query ombra | Ombre bloccate |
| --- | --- | --- | --- | --- |
| Prima | 32.400 | 4.105 | 19.277 | 1.516 (7,9%) |
| Terza | 32.400 | 805 | 18.190 | 1.616 (8,9%) |
| Dall'alto | 32.400 | 1.374 | 24.995 | 5.667 (22,7%) |

Nel replay iniziale i gruppi ombra costavano 6,71–9,99 ms seriali, quelli primari 9,72–13,68 ms e quelli riflessi 0,23–1,40 ms. È una motivazione per esaminare le ombre; non una percentuale certificata del frame. Raggruppare i raggi cambia cache e ordine rispetto alla composizione vera, quindi **non sommare questi batch per ricostruire un frame time**.

I conteggi e le risposte sono identici dopo la modifica. Prima/dopo sono sessioni distinte: anche i gruppi primari invariati cambiano durata per le condizioni della macchina. Per attribuire il beneficio alla modifica usiamo il confronto abbinato sotto, non la semplice differenza dei replay. Dati grezzi in `evidence/r4/profile-before/` e `profile-after/`.

## Confronto CPU abbinato

`examples/shadow_bench.rs` confronta nello stesso eseguibile il default nearest-hit e la query specializzata. Entrambi usano lo stesso snapshot e compositore; solo il percorso ombra cambia. Varianti alternate all'interno di ciascun frame, ordine invertito a ogni frame/ripetizione, storie e pool separati; un solo compositore attivo alla volta.

Tre ripetizioni, worker 1 e 8, tre camere, Cloth 240×135, 12 warm-up e 120 frame misurati per variante: 36 run e 4.320 frame misurati. Il tempo include preparazione snapshot, composizione e rilascio del buffer; esclude avvio pool e GPU. Nessuna build o altra prova del laboratorio in parallelo.

Host: Windows x64, Ryzen 7 3800X, capacità stimata 16, Rust 1.97.1 MSVC release. Le mediane seguenti sono mediane dei p50 delle tre ripetizioni, non percentili di campioni aggregati.

| Worker | Camera | Default nearest-hit | Query ombra dedicata |
| --- | --- | --- | --- |
| 1 | Prima | 28,62 ms | 28,09 ms |
| 1 | Terza | 26,06 ms | 25,14 ms |
| 1 | Dall'alto | 29,65 ms | 28,31 ms |
| 8 | Prima | 5,35 ms | 5,24 ms |
| 8 | Terza | 4,79 ms | 4,80 ms |
| 8 | Dall'alto | 5,53 ms | 5,43 ms |

Riduzione del p50 mediano di circa 1,9–4,5% in seriale, circa 2% in prima/dall'alto con 8 worker. In terza/8 non c'è un guadagno dimostrato: lo scarto è leggermente sfavorevole. Non tutte le ripetizioni migliorano tutti i percentili. Non dichiariamo significatività statistica o scaling universale.

I picchi rimangono: nella terza ripetizione prima/8, p95 70,16→64,94 ms. In terza/8/ripetizione 3 il p95 peggiora 12,06→12,61 ms. Nessuna causa di questi picchi è stata misurata e nessun campione è stato eliminato. La maggior parte dei raggi ombra è libera, quindi deve ancora attraversare l'ambiente; l'attore/mantello conserva il costo precedente. Questo spiega il limite strutturale dell'intervento, senza attribuire al codice tutta la variabilità del desktop.

Tutti i p50/p95 e gli intervalli fra ripetizioni sono in [summary.md](evidence/r4/summary.md), con JSON e CSV grezzi. Sono tempi CPU, non GPU time, utilizzo hardware o FPS della finestra. Non sono misure su altri processori.

## Verifiche

- Fmt, clippy all-targets, suite release e build release PASS. 73 esecuzioni di test, comprese copie storiche importate come oracolo; non 73 funzionalità distinte. Solo avviso della cache incrementale sui collegamenti fisici sostituiti da copie.
- 216 combinazioni fra compositore/mondo R3 congelati e R4: tre camere, quattro stage, worker 1/2/8, sei tick con movimento camera e reset della storia. Byte delle celle e statistiche temporali identici.
- 28.800 raggi deterministici e otto soglie ciascuno: 230.400 confronti del booleano con il nearest-hit congelato; più test mirati su emissivi, parità, limiti adiacenti rappresentabili, NaN/infinito, origine interna e direzione non unitaria.
- Vulkan e DirectX 12: 12 combinazioni camera/stage ciascuno, scena congelata e animazione/pan. Tutte le 24 PNG identiche R3/Vulkan; zero differenze statiche con/senza storia. Smoke Vulkan a finestra per 2 s richiesti PASS, senza trasformarlo in un benchmark FPS.
- Baseline R3: 13/13 hash di sorgenti/Cargo/eseguibile preservati in `evidence/r4/baseline/`. Attore, GPU e shader invariati. Nessuna nuova validazione artistica continua del filmato dichiarata; questa tranche conserva l'aspetto già visto dall'utente.

## Riproduzione

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --release --all-targets
cargo build --locked --release --bins --example query_profile --example shadow_bench
.\target\release\examples\query_profile.exe --output evidence\new-query-profile
.\target\release\examples\shadow_bench.exe
.\scripts\verify-r4.ps1
python .\scripts\summarize-queries.py
```

Il profilo iniziale va conservato: la build attuale usa già il percorso ottimizzato. Lo script di sintesi richiede le baseline locali e confronta con le catture R3. Prima di una nuova revisione conservare questi risultati e usare una nuova cartella; non sovrascrivere una prova storica per confrontare codice diverso.

## Prossima consegna

Un provino visivo circoscritto su figura/mantello e leggibilità dei materiali, con luce e camera controllate. Le query primarie restano un costo importante, ma i dati raccolti non impongono di rimandare ancora la qualità o il riuso del motore. Seguiranno dati dei contenuti e un secondo client indipendente, poi consolidamento e hardware aggiuntivo. Criteri di alpha/v1 e spiegazione del rapporto fra gioco, motore e strumenti in [ENGINE_MILESTONES.md](../ascii-game/docs/ENGINE_MILESTONES.md).
