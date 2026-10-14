# Material Room — secondo client del renderer

Questa applicazione dipende da `ascii-render`, non da `ascii-engine-lab`. Carica oggetti, materiali, luce e camere da un JSON validato; prepara uno snapshot immutabile per fotogramma e lo passa al compositore. Non importa figura o cortile BLACKWATER. La versione 2 dei dati espone superfici locali, conduttori e ambiente limitato; la versione 1 conserva il percorso R6. Forme e materiali sono dimostrativi, non il target artistico finale né una API stabile.

## Compilazione e uso

Dalla radice del repository `ascii-engine-lab`, anche in un checkout nuovo, con toolchain Rust e linker nativo installati:

```sh
cargo build --locked --release -p ascii-material-room
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room.json --validate
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json --validate
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json --headless --camera all --frames 60 --profile balanced --workers 8 --output evidence/my-material-room-v2
```

La prima compilazione richiede accesso a crates.io o dipendenze già in cache. `--offline` serve solo con cache completa. Windows MSVC richiede i Build Tools C++; runtime verificato sullo sviluppo Windows x64, AMD RX 6700 XT, Vulkan e DX12. Altri sistemi/hardware non sono ancora collaudati. Un checkout pulito sulla stessa macchina verifica la guida, non una seconda piattaforma. Requisiti e limiti GPU sono in [PORTABILITY](../../PORTABILITY.md). `--validate` termina dopo il caricamento e non richiede una GPU o una finestra.

Il default CLI e `Avvia-materiali.cmd` rimangono sul campione v1; per v2 passare esplicitamente `--scene clients/material-room/assets/material-room-v2.json`. Il launcher Windows costruisce il programma solo se manca. Dopo modifiche al codice ricompilare; dopo modifiche al JSON basta **R**. Se si copia l'eseguibile altrove, passare `--scene` con il percorso del JSON: il percorso predefinito è quello del checkout usato nella compilazione. L'atlante dei caratteri è incorporato, non richiede file font esterni.

Su Windows, [Avvia-alpha.cmd](../../Avvia-alpha.cmd) avvia direttamente v2 e ricompila sempre con `--locked --release`, così l'eseguibile segue i sorgenti correnti. Accetta gli stessi argomenti aggiuntivi della CLI.

Comandi: **1/2/3** prima/terza/alto; **Q/E** orbita; **WASD** sposta osservatore; **Z/X** altezza; **Spazio** pausa; **R** rilegge JSON; **H** etichette; **Esc** chiude. Una ricarica valida sostituisce insieme tutti i dati, ripristina la camera del modo corrente dalla nuova scena e azzera la history dei glifi; il tempo di animazione prosegue. Un file non valido conserva documento, camera, tempo e history precedenti e stampa la diagnosi. I tasti 1/2/3 ripartono dalla camera dichiarata per quel modo. Le camere sono osservatori, non controlli di un personaggio giocabile.

`--profile economy|balanced|detail`, `--workers auto|N` e `--backend auto|vulkan|dx12|metal|gl` usano gli stessi budget del laboratorio. `--sequence 180 --camera all` salva 180 PNG per vista a tick simulati di 1/30 s; `--time` sceglie il tempo iniziale; `--seconds` limita lo smoke a finestra. `--frames` senza `--sequence` avanza comunque lo stato di 1/30 s per frame, misura tutti i fotogrammi e salva il primo e l'ultimo. `--orbit-speed` è solo headless, accetta da -2 a 2 radianti/s e aggiunge un'orbita dalla posa iniziale: `frame / 30 * speed`. Il default 0 lascia ferma la camera. Le catture sono GPU reali, non immagini ricostruite con un altro renderer.

Per una sequenza con camera e oggetti in movimento nelle tre viste:

```sh
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json --sequence 180 --camera all --orbit-speed 0.025 --profile detail --workers 8 --output evidence/my-material-room-motion
```

## Dati comuni e compatibilità v1

[assets/material-room.json](assets/material-room.json) è l'esempio completo: 7 oggetti, 4 materiali, luce mobile e pannello che trasla/ruota. Le sue forme sono dimostrative. Il pannello è rigido: non è una simulazione del tessuto.

| Campo | Contenuto |
| --- | --- |
| `version`, `name` | Versione intera 1 o 2; nome non vuoto, massimo 80 caratteri senza controlli |
| `materials` | Fino a 32 definizioni: `id`, `name`, `pigment`, `pattern`; array vuoto ammesso |
| `objects` | Fino a 128 definizioni: `id`, `name`, `shape`, `material`, `position`, `half_extents`; opzionali `rotation_y`, `motion`; array vuoto ammesso |
| `light` | `position`; opzionale `motion.translation` |

Materiale: `id` intero da 32 a u32::MAX; `pigment` tre numeri lineari RGB 0..1; `pattern.kind` uno di `plain`, `stone`, `wood`, `cloth`; `pattern.scale` 0.0001..64; `pattern.palette` da 1 a 16 caratteri ASCII stampabili. I nomi sono metadati. I pattern scelgono segni da coordinate locali, quindi seguono l'oggetto; il pigmento è uniforme per materiale. Non sono texture realistiche o un sistema PBR completo.

Oggetto: `id` intero 1..u32::MAX univoco; `material` deve esistere; `shape` è `box` o `ellipsoid`. `position` è un vettore con componenti fra -10000 e 10000. `half_extents` contiene semiassi positivi 0.0001..10000 (raggi per ellissoidi). `rotation_y` è in radianti, fra -1024 e 1024.

Movimento opzionale: `translation` contiene `amplitude` vettore ±1000, `speed` ±64 radianti/s, `phase` opzionale ±10000 radianti. `yaw` ha `amplitude` ±1024 radianti, `speed` e `phase` come sopra. La luce ammette solo traslazione. Posizione e yaw aggiungono `amplitude * sin(speed * time + phase)` alla posa base. Non è una timeline o fisica.

File UTF-8 massimo 1 MiB; campi sconosciuti anche annidati, chiavi JSON duplicate anche con escape equivalenti, ID duplicati, riferimenti mancanti, valori fuori intervallo e versioni non supportate vengono respinti. Un tempo finito è requisito di `snapshot(time)`; app e CLI lo controllano. I nomi dei campi negli errori individuano il dato invalido.

I file v1 continuano a usare materiali e illuminazione legacy e le camere R6 del client. `surface`, `cameras`, `environment` e `light.color` sono campi v2: inserirli in un file v1 produce un errore, non una migrazione implicita. Il campione v1 originale è mantenuto per regressione.

## JSON v2: superfici, conduttori, ambiente e camere

[assets/material-room-v2.json](assets/material-room-v2.json) contiene la scena completa con superfici dichiarate, un campione metallico e camere proprie. Riusa contenuti semplici del client e aggiunge un ellissoide sul supporto: non importa il metallo lavorato, la pietra incisa o la figura dei provini R7–R10.

Ogni materiale v2 richiede `surface`. Esempio conduttore:

```json
"surface": {
  "roughness": 0.28,
  "f0": 0.04,
  "occlusion": 1.0,
  "metallic": { "f0": [0.72, 0.76, 0.8], "metalness": 1.0 }
}
```

`roughness`, `f0` e `occlusion` sono scalari 0..1: rugosità percettiva, riflettanza dielettrica a incidenza normale e visibilità locale dichiarata dall'autore. `metallic` è opzionale; il suo `f0` è l'endpoint conduttore RGB lineare 0..1 e `metalness` miscela dielettrico/conduttore da 0 a 1. Zero conserva esattamente la risposta dielettrica; uno elimina la componente diffusa. I parametri sono uniformi per materiale e usano la normale geometrica, senza una nuova mappa di dettaglio. Ombre metalliche scure sono un limite del trasporto approssimato, non un errore da correggere aumentando il pigmento.

I pattern v2 riducono il dettaglio quando l'impronta della cella cresce: regioni fini, bande più larghe e infine un segno uniforme. La stessa classificazione alimenta glifo e chiave della history. È un filtro discreto delle grammatiche del client, non antialiasing geometrico completo o una nuova libreria di texture.

`light.color` è opzionale, RGB lineare con canali 0..16; senza il campo usa `[1.0, 0.58, 0.23]`. La luce rimane una sola e conserva posizione/movimento del formato comune.

`environment`, opzionale alla radice, contiene tutti e tre questi campi:

```json
"environment": {
  "diffuse_irradiance": [0.35, 0.42, 0.5],
  "specular_radiance": [0.5, 0.62, 0.8],
  "max_distance": 12.0
}
```

I due vettori sono rispettivamente irradianza diffusa E e radianza speculare, con canali lineari 0..16; non sono due colori ambientali intercambiabili. Il renderer divide E per pi e applica Fresnel e la separazione diffuso/conduttore. Il client fornisce un ambiente costante, non un'immagine HDR o una sonda direzionale. `max_distance` è il raggio 0..100 per query locali: zero disattiva quelle query e conserva l'ambiente distante. Omettere `environment` seleziona invece il contributo ambientale locale precedente R9. Presente, l'ambiente sostituisce quel contributo.

`cameras` richiede tutte le viste `first`, `third`, `overhead`, ciascuna con quattro campi:

```json
"first": {
  "eye": [0.0, 1.9, 6.2],
  "target": [0.0, 1.3, -0.4],
  "fov_y_degrees": 55.0,
  "ortho_half_height": 4.2
}
```

`eye` e `target` hanno componenti ±10000, devono distare più di 0.0001 unità e definire una direzione non parallela o quasi parallela all'asse Y. La base della camera usa Y come alto del mondo. `fov_y_degrees` è l'angolo verticale 1..179 gradi, usato da prima e terza persona; `ortho_half_height` è la semialtezza ortografica 0.01..10000, usata dall'alto. Entrambi sono obbligatori e validati in ogni vista. Orbita, offset e zoom sono stato dell'host: in prima persona l'orbita mantiene fermo l'occhio, nelle altre viste mantiene fermo il target.

Gli ID sono assegnati dall'autore e non dalla posizione nell'array. In v2 un'uguaglianza esatta di distanza fra superfici è risolta dall'ID oggetto minore; riordinare materiali e oggetti conserva il risultato. La v1 mantiene invece l'ordine originale per non cambiare i vecchi output. Riutilizzare un ID per un contenuto diverso richiede un reset della history: il reload dell'host lo esegue sempre.

Per migrare un proprio file, farne una copia, portare `version` a 2, aggiungere `surface` a ogni materiale e tutte le `cameras`; poi aggiungere facoltativamente `light.color`, `metallic` e `environment`. Verificare con `--validate` prima dell'anteprima. L'immagine può cambiare: v2 attiva una risposta locale e il filtro dei pattern, quindi la migrazione non promette parità visiva con v1. Per conservare l'aspetto legacy mantenere la versione 1.

## Confine con il motore

`SceneDocument::load/from_json` valida l'intero documento prima di renderlo disponibile. `version()` identifica il formato e `source()` restituisce il JSON effettivamente validato. `camera(mode)` crea lo stato iniziale della vista dai dati del client; `snapshot(time)` prepara trasformazioni e luce immutabili. Documento e snapshot già esistenti non vengono modificati quando si carica una nuova scena.

Il client passa lo stesso tempo a `snapshot` e `Composer::compose`. Raggi primari, ombre e riflessi interrogano quello snapshot. V1 usa `trace`, `light_position`, `pigment`, `glyph` e `glyph_history_key`; v2 espone anche `local_materials`, `material_sample`, `metallic_sample`, `environment_sample`, `light_color`, `glyph_sample` e `glyph_sample_key`. Le ombre usano il default `Scene::occluded`. Un tempo diverso nelle query ricostruisce uno snapshot temporaneo corretto, ma è più costoso e non è il flusso normale.

Il compositore e la presentazione GPU sono in `crates/ascii-render`; dati, grammatica dei pattern, simulazione e controlli della stanza appartengono al client. Per aggiungere un oggetto supportato o cambiare colore, dimensioni, superficie, ambiente o inquadratura bastano i dati. Nuovi tipi di geometria o risposta materiale richiedono codice e una prova dedicata. Nessuna scena a mesh viene renderizzata e poi convertita in ASCII.

Il trasporto d'ambiente usa una sola query lungo la normale per la visibilità diffusa e al massimo un rimbalzo speculare locale. Un hit locale esclude la sorgente distante; la rugosità attenua il contributo e può produrre riflessi troppo scuri o troppo netti. Non è GI/IBL completa, scattering multiplo o PBR calibrato. `FrameStats.reflected` conta ancora solo l'acqua legacy. Il client usa ID materiali >=32 e non espone i ruoli riservati acqua/emissivo. Non aggiunge fisica del tessuto, fuoco, distruzione, gameplay o editor universale.

## Verifiche del contratto

```sh
cargo test --locked -p ascii-material-room --all-targets
cargo test --locked --workspace --all-targets
```

[tests/data_contract.rs](tests/data_contract.rs) confronta il produttore v1 con il sorgente e JSON congelati di fine R10: celle e statistiche bit per bit nelle tre camere, quattro stage, tempi ripetuti/in movimento, uno e otto worker e reset della history. Altri test verificano indipendenza degli snapshot, determinismo v2, riordino con ID stabili, modifiche di materiali/luce/ambiente/camere solo dai dati e rifiuto degli errori annidati. Il confronto usa il compositore corrente su entrambi i produttori; le regressioni storiche del renderer sono coperte separatamente nel workspace. Le prove dell'host coprono la ricarica valida/invalida e il reset di camera/history.

## Evidenze

Ogni esecuzione headless salva `run.json`, la **scena validata effettivamente caricata**, versione dati, capacità GPU, camere iniziali, velocità d'orbita, griglia, worker, p50/p95/p99 CPU e submission/attesa, contatori temporali e CSV. Dodici warmup CPU+GPU esclusi; PNG fuori dai timer. L'attesa non è un timestamp GPU o FPS di presentazione. Catture e scritture possono influenzare la schedulazione. Statico identico non prova da solo assenza di sfarfallio in movimento, e 30 tick simulati/s non sono una misura di 30 FPS. Risultati storici R6 in [REUSE.md](../../REUSE.md); stato corrente e limiti verificati in [PROJECT_STATUS.md](../../PROJECT_STATUS.md), criteri della tranche in [ENGINE_PATH.md](../../ENGINE_PATH.md).
