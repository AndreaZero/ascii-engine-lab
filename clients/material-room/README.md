# Material Room — secondo client del renderer

Questa applicazione dipende da `ascii-render`, non da `ascii-engine-lab`. Carica oggetti, materiali e luce dal JSON; prepara uno snapshot immutabile per fotogramma e lo passa al compositore. Non importa figura o cortile BLACKWATER. È un esempio di integrazione e creazione dei contenuti, non il provino della qualità artistica finale.

## Compilazione e uso

Dalla radice del workspace Cargo, con toolchain Rust e linker nativo installati:

```sh
cargo build --locked --release -p ascii-material-room
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room.json
cargo run --locked --release -p ascii-material-room -- --validate
cargo run --locked --release -p ascii-material-room -- --headless --camera all --frames 60 --profile balanced --output evidence/my-material-room
```

La prima compilazione richiede accesso a crates.io o dipendenze già in cache. `--offline` serve solo con cache completa. Windows MSVC richiede i Build Tools C++; runtime verificato qui su Windows x64, AMD RX 6700 XT, Vulkan e DX12. Altri sistemi/hardware non sono ancora collaudati. I requisiti e limiti GPU sono quelli di [PORTABILITY](../../PORTABILITY.md).

Windows: `Avvia-materiali.cmd` alla radice costruisce il programma se manca. Dopo modifiche al codice ricompilare esplicitamente; dopo modifiche al JSON basta **R**. Se si copia l'eseguibile altrove, passare `--scene` con il percorso del JSON: il default incorporato punta al checkout usato nella compilazione. L'atlante dei caratteri è incorporato, non richiede file font esterni.

Comandi: **1/2/3** prima/terza/alto; **Q/E** orbita; **WASD** sposta osservatore; **Z/X** altezza; **Spazio** pausa; **R** rilegge JSON; **H** etichette; **Esc** chiude. Un file non valido lascia attiva l'ultima scena valida e stampa la diagnosi. Le camere sono osservatori, non controlli di un personaggio giocabile.

`--profile economy|balanced|detail`, `--workers auto|N` e `--backend auto|vulkan|dx12|metal|gl` usano gli stessi budget del laboratorio. `--sequence 180 --camera all` salva 180 PNG per vista a tick simulati di 1/30 s; `--time` sceglie il tempo iniziale; `--seconds` limita lo smoke a finestra. `--frames` senza `--sequence` misura tutti i fotogrammi e salva il primo e l'ultimo. Le catture sono GPU reali, non immagini ricostruite con un altro renderer.

## JSON v1

[assets/material-room.json](assets/material-room.json) è l'esempio completo: 7 oggetti, 4 materiali, luce mobile e pannello che trasla/ruota. Le sue forme sono dimostrative. Il pannello è rigido: non è una simulazione del tessuto.

| Campo | Contenuto |
| --- | --- |
| `version`, `name` | Versione intera 1; nome non vuoto, massimo 80 caratteri senza controlli |
| `materials` | Fino a 32 definizioni: `id`, `name`, `pigment`, `pattern`; array vuoto ammesso |
| `objects` | Fino a 128 definizioni: `id`, `name`, `shape`, `material`, `position`, `half_extents`; opzionali `rotation_y`, `motion`; array vuoto ammesso |
| `light` | `position`; opzionale `motion.translation` |

Materiale: `id` intero da 32 a u32::MAX; `pigment` tre numeri 0..1; `pattern.kind` uno di `plain`, `stone`, `wood`, `cloth`; `pattern.scale` 0.0001..64; `pattern.palette` da 1 a 16 caratteri ASCII stampabili. I nomi sono metadati. I pattern scelgono segni da coordinate locali, quindi seguono l'oggetto; il pigmento è ancora uniforme per materiale. Non sono texture realistiche o un sistema PBR.

Oggetto: `id` intero 1..u32::MAX univoco; `material` deve esistere; `shape` è `box` o `ellipsoid`. `position` è un vettore con componenti fra -10000 e 10000. `half_extents` contiene semiassi positivi 0.0001..10000 (raggi per ellissoidi). `rotation_y` è in radianti, fra -1024 e 1024.

Movimento opzionale: `translation` contiene `amplitude` vettore ±1000, `speed` ±64 radianti/s, `phase` opzionale ±10000 radianti. `yaw` ha `amplitude` ±1024 radianti, `speed` e `phase` come sopra. La luce ammette solo traslazione. Posizione e yaw aggiungono `amplitude * sin(speed * time + phase)` alla posa base. Non è una timeline o fisica.

File UTF-8 massimo 1 MiB; campi sconosciuti, chiavi JSON duplicate, ID duplicati, riferimenti mancanti, valori fuori intervallo e versioni diverse vengono respinti. Un tempo finito è requisito di `snapshot(time)`; app e CLI lo controllano. I nomi dei campi negli errori individuano il dato invalido.

## Confine con il motore

`SceneDocument::load/from_json` valida i dati. `snapshot(time)` prepara le trasformazioni. Il client passa quello stesso tempo a `Composer::compose`; `Scene::trace`, `light_position`, `pigment`, `glyph`, `glyph_history_key` forniscono geometria e semantica. Le ombre usano il default `Scene::occluded` sullo stesso snapshot. I pattern producono anche la chiave strutturale della history per non trattenere un carattere appartenente a un'altra regione. Un tempo diverso nelle query ricostruisce uno snapshot temporaneo corretto, ma è più costoso e non è il flusso normale.

La scelta dei caratteri e la presentazione GPU sono in `crates/ascii-render`; dati e simulazione di questa stanza appartengono al client. Per aggiungere un oggetto o cambiare colore/dimensioni/materiale bastano i dati. Nuovi tipi di geometria o risposta materiale richiedono codice e una prova dedicata.

Limiti ereditati: una luce, illuminazione/ambiente fissati nel compositore, ruoli speciali acqua/emissivo con ID riservati, camere iniziali e stage del laboratorio. Questo client usa ID >=32 e non espone quei ruoli. Non dimostra che la libreria sia già un motore universale o una API stabile.

## Evidenze

Ogni esecuzione headless salva `run.json`, la **scena validata effettivamente caricata**, capacità GPU, camere, griglia, worker, p50/p95/p99 CPU e submission/attesa, contatori temporali e CSV. Dodici warmup CPU+GPU esclusi; PNG fuori dai timer. L'attesa non è un timestamp GPU o FPS di presentazione. Catture e scritture possono influenzare la schedulazione. Statico identico non prova da solo assenza di sfarfallio in movimento. Risultati della consegna in [REUSE.md](../../REUSE.md).
