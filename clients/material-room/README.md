# Material Room — secondo client del renderer

Questa applicazione dipende da `ascii-render`, non da `ascii-engine-lab`. Carica oggetti, materiali, luce e camere da un JSON validato; prepara uno snapshot immutabile per fotogramma e lo passa al compositore. Non importa figura o cortile BLACKWATER. La versione 2 dei dati espone superfici locali, conduttori e ambiente limitato; la versione 1 conserva il percorso R6. Forme e materiali sono dimostrativi, non il target artistico finale né una API stabile.

R14 aggiunge [transazioni di occupazione](#occupazione-dinamica-r14) nel client: inserimento, rimozione e trasformazione assoluta, con identità di istanza e snapshot precedenti immutabili. L'API e la timeline `--updates` sono verificate con test, ricostruzione indipendente e runtime nelle tre viste. Il [report R14](../../DYNAMIC_OCCUPANCY.md) riporta risultati e limiti di costo, distinti dalla qualità artistica.

## Authoring pratico R15

`author` crea e modifica un progetto JSON unico con scena v2, timeline v1 e
controlli indipendenti delle tre camere. `--project` lo apre nell'anteprima:
Tab seleziona gli oggetti iniziali, frecce/PagSu/PagGiù li spostano, Ctrl+D
duplica, Canc rimuove e Ctrl+S salva. I tasti 1/2/3 conservano le camere;
ogni modifica alla scena riparte in pausa al tick 0. R rifiuta di ricaricare
modifiche non salvate; Esc e la chiusura le proteggono, Maiusc+Esc le scarta
esplicitamente. Non c'è salvataggio automatico.

```sh
cargo build --locked --release -p ascii-material-room --bins
cargo run --locked --release -p ascii-material-room --bin author -- new my-room.json
cargo run --locked --release -p ascii-material-room -- --project my-room.json
```

La [guida di authoring](AUTHORING.md) contiene comandi create/import/inspect,
modifiche di oggetti/materiali/camere, una patch coordinata con gli eventi,
salvataggio e conflitti. Tutti gli eventi futuri vengono validati prima di
accettare un candidato. Il progetto salva i dati iniziali, non lo stato dopo
il playback; non aggiunge un editor universale. Per le verifiche R15
consultare [PROJECT_STATUS](../../PROJECT_STATUS.md) per i risultati confermati.

## Query R13

R13 verifica un helper box con limite esatto, ma la sua integrazione sperimentale
riduce assi e peggiora i tempi CPU. Il percorso ordinario conserva quindi
algoritmo e ombre R12, senza una nuova modalità da scegliere. JSON, materiali,
trasformazioni, ordine e pareggi restano invariati. Nessuna gerarchia spaziale
o accelerazione runtime dichiarata. Risultati e limiti in
[QUERY_ACCELERATION](../../QUERY_ACCELERATION.md).

I metodi `Snapshot::trace_bounded_counted` e `Snapshot::occluded_bounded_counted`
esercitano esplicitamente il candidato e ne restituiscono contatori locali,
fuori dalle misure temporali. Non misurano il lavoro del normale percorso
`Scene`, che non li invoca. Le ombre any-hit dell'esperimento sono valide solo
per i materiali >=32 di questo client. Dopo la compilazione, l'esempio
`cargo run --locked --release -p ascii-material-room --example query_audit`
esercita i tre JSON e le tre visuali; leggere il report per il protocollo.

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

Il default CLI e `Avvia-materiali.cmd` rimangono sul campione v1; per v2 passare esplicitamente `--scene clients/material-room/assets/material-room-v2.json` oppure aprire un progetto con `--project`. Il launcher Windows costruisce il programma solo se manca. Dopo modifiche al codice ricompilare; dopo modifiche al JSON basta **R**, salvo modifiche non salvate in un progetto. Il default CLI v1 e l'atlante dei caratteri sono incorporati: un eseguibile spostato non legge il checkout usato nella compilazione e non richiede file font esterni. Per contenuti propri passare il percorso di `--scene` o `--project`.

Su Windows, [Avvia-alpha.cmd](../../Avvia-alpha.cmd) avvia direttamente v2 e ricompila sempre con `--locked --release`, così l'eseguibile segue i sorgenti correnti. Accetta gli stessi argomenti aggiuntivi della CLI.

Comandi: **1/2/3** prima/terza/alto; **Q/E** orbita; **WASD** sposta osservatore; **Z/X** altezza; **Spazio** pausa; **R** rilegge JSON; **H** etichette; **C** cambia campionamento primario; **P** attiva/disattiva il percorso camera dello studio di copertura; **Esc** chiude. Una ricarica valida sostituisce insieme i dati, ripristina la camera del modo corrente e azzera la history dei glifi. Senza `--updates` il tempo di animazione prosegue, come nel comportamento precedente. Con `--updates`, R valida insieme scena e intera sequenza degli eventi, crea un nuovo stato runtime e riparte dal tempo iniziale `--time` (default zero), applicando gli eventi già dovuti a quell'istante. Un errore, anche in un evento futuro, conserva documento, playback, camera, tempo e history precedenti e stampa la diagnosi. I tasti 1/2/3 ripartono dalla camera dichiarata per quel modo. Anche C e P azzerano la history. Le camere sono osservatori, non controlli di un personaggio giocabile.

`--profile economy|balanced|detail`, `--workers auto|N` e `--backend auto|vulkan|dx12|metal|gl` usano gli stessi budget del laboratorio. `--sequence 180 --camera all` salva 180 PNG per vista a tick simulati di 1/30 s; `--time` sceglie il tempo iniziale; `--seconds` limita lo smoke a finestra. `--frames` senza `--sequence` avanza comunque lo stato di 1/30 s per frame, misura tutti i fotogrammi e salva il primo e l'ultimo. `--orbit-speed` è solo headless, accetta da -2 a 2 radianti/s e aggiunge un'orbita dalla posa iniziale: `frame / 30 * speed`. Il default 0 lascia ferma la camera. Le catture sono GPU reali, non immagini ricostruite con un altro renderer.

Per una sequenza con camera e oggetti in movimento nelle tre viste:

```sh
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/material-room-v2.json --sequence 180 --camera all --orbit-speed 0.025 --profile detail --workers 8 --output evidence/my-material-room-motion
```

## Copertura geometrica opzionale

`--sampling center|conservative9` sceglie il campionamento primario; il default `center` conserva il percorso R11. `conservative9` interroga la scena in nove punti per cella: centro e griglia 3×3 con offset di ±1/3 di cella. Sceglie il gruppo oggetto/materiale più vicino per profondità lungo la direzione della camera; se il centro appartiene a quel gruppo, lo preferisce. Le uguaglianze esatte sono risolte da ID oggetto, ID materiale e indice del campione. Viene ombreggiata una sola superficie e prodotto un solo glifo, con impronta materiale di una cella intera. Ombre e riflessi partono dalla superficie scelta: non vengono eseguiti nove passaggi completi di illuminazione.

Questo percorso può recuperare aste e bordi fra raggi centrali, ma può anche allargare una silhouette fino alla cella e perdere geometria fra i nove punti. Non misura la copertura d'area esatta, non miscela RGB e non elimina tutto l'aliasing. La history considera anche campione scelto e maschera dei campioni appartenenti al gruppo: un cambio di provenienza libera il glifo precedente. Non è una garanzia generale di stabilità percettiva in movimento.

[assets/coverage-room-v2.json](assets/coverage-room-v2.json) è un carico tecnico con aste, traversi, bordo curvo e occlusore mobile. I glifi distinti permettono di riconoscere il soggetto dalle celle effettivamente prodotte. `--camera-path coverage` applica alla camera dei dati un percorso periodico di sei secondi: traslazione laterale, allontanamento, piccola orbita e variazione di zoom nella vista dall'alto. Il default `static` lascia invariata la posa, salvo i controlli manuali o l'eventuale `--orbit-speed` headless. Profilo, densità e campionamento non cambiano durante il percorso.

Per catturare lo stesso percorso nelle tre viste:

```sh
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/coverage-room-v2.json --sampling conservative9 --camera-path coverage --sequence 180 --camera all --profile detail --workers 8 --output evidence/my-coverage-nine
```

Per il confronto ripetere con `--sampling center` e una diversa directory di output, mantenendo invariati gli altri parametri. Su Windows [Avvia-copertura.cmd](../../Avvia-copertura.cmd) ricompila sempre in release e apre questa scena con nove campioni e percorso attivo; C e P consentono il confronto interattivo.

Per misurare snapshot e composizione senza creare una GPU:

```sh
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/coverage-room-v2.json --cpu-only --sampling conservative9 --camera-path coverage --camera all --frames 180 --profile detail --workers 8 --output evidence/my-coverage-cpu
```

`--cpu-only` implica headless, esclude catture e audit centrale e rifiuta `--sequence`. Non misura presentazione o tempo GPU. `primary_rays` conta le query primarie tentate (una o nove per cella, comprese quelle vuote), escludendo ombre e riflessi; `alternate_primary` conta le celle la cui superficie scelta viene da un punto non centrale, anche quando il glifo risulta oscuro. Nessuno dei due contatori misura l'area corretta o la qualità visiva. L'esempio `coverage_audit` confronta le celle con un riferimento geometrico denso fuori dai timer, riportando anche massa mancata ed eccesso; il riferimento campionato resta approssimato.

```sh
cargo run --locked --release -p ascii-material-room --example coverage_audit -- --output evidence/my-coverage-audit
```

L'audit predefinito usa 13 istanti inclusivi fra 0 e 6 secondi, griglia 80×45 e riferimento 16×16 punti per cella. A inizio, metà e fine percorso ripete il riferimento a 32×32 per misurarne la convergenza. Il JSON conserva risultati per oggetto, assenze agli istanti campionati, transizioni e confronto history/fresh; riporta separatamente conteggi delle query del client R11 e del carico di copertura. Un budget esplicito di 250 milioni di raggi limita le combinazioni richieste. Gli istanti discreti non dimostrano assenza di errori intermedi, né il riferimento finito costituisce una verità geometrica esatta.

## Occupazione dinamica R14

`RuntimeScene`, in [src/runtime.rs](src/runtime.rs), è un proprietario mutabile del documento usato fra i frame. `apply_transaction(expected_revision, operations)` prepara un documento candidato e lo valida con `SceneDocument` prima di pubblicare insieme dati, identità e revisione. Una query non modifica mai la simulazione. Materiali, luce e camere conservano il formato v1/v2; questa API modifica soltanto gli oggetti e non introduce una gerarchia spaziale, fisica o distruzione completa.

| Operazione Rust | Effetto |
| --- | --- |
| `Operation::Insert { object }` | Inserisce un `serde_json::Value` con lo schema oggetto esistente; ID libero, materiale e tutti i campi devono essere validi |
| `Operation::Remove { handle }` | Rimuove l'istanza viva identificata dall'handle |
| `Operation::SetTransform { handle, transform }` | Sostituisce `position`, `half_extents` e `rotation_y`; rimuove l'eventuale `motion`, quindi la posa è assoluta |

`ObjectHandle { namespace, id, generation }` distingue anche istanze successive dello stesso ID. Un oggetto superstite mantiene l'handle durante trasformazioni e modifiche ad altri oggetti; una rimozione seguita da reinserimento assegna una generazione nuova. Gli handle di un altro runtime o di una vecchia istanza sono respinti. `SceneRevision { namespace, revision }` protegge la transazione da uno stato atteso ormai superato. Namespace e token appartengono all'esecuzione: il riferimento persistente nei dati rimane l'ID dichiarato dall'autore.

Ogni batch ammette fino a 128 operazioni e una sola operazione per ID; il documento finale contiene al massimo 128 oggetti. Per rimuovere e reinserire lo stesso ID servono due commit, anche senza un frame intermedio. Gli inserimenti v1 vengono aggiunti dopo i superstiti, conservando l'ordine precedente; v2 mantiene la priorità per ID nelle uguaglianze di distanza. Un errore lascia invariati documento, revisione, generazioni ed epoch. Batch vuoti e trasformazioni identiche per bit `f32`, senza `motion`, non avanzano lo stato; togliere un campo `motion` presente è invece una modifica, anche se la sua ampiezza è zero. I contatori non vengono riciclati: un esaurimento viene respinto prima della pubblicazione.

Il risultato `CommitReceipt` contiene `changed`, la nuova `revision` e gli handle `inserted`. `handle(id)`, `handles()`, `source()`, `camera(mode)` e `snapshot(time)` permettono di usare il runtime direttamente, senza file eventi. Esempio con il campione v2:

```rust
use ascii_material_room::{
    Operation, RuntimeScene, RuntimeSnapshot, SceneDocument, Transform,
    DEFAULT_SCENE_V2_JSON,
};
use glam::Vec3;

fn moved_snapshot() -> Result<RuntimeSnapshot, String> {
    let document = SceneDocument::from_json(DEFAULT_SCENE_V2_JSON)?;
    let mut state = RuntimeScene::new(document)?;
    let handle = state.handle(2).ok_or("object 2 is absent")?;
    let expected = state.revision();
    state.apply_transaction(expected, &[Operation::SetTransform {
        handle,
        transform: Transform {
            position: Vec3::new(-2.0, 1.25, -1.8),
            half_extents: Vec3::new(0.4, 1.25, 0.45),
            rotation_y: 0.2,
        },
    }])?;
    Ok(state.snapshot(1.0))
}
```

Passare lo snapshot restituito a `Composer::compose` con lo stesso tempo. `RuntimeSnapshot` conserva il normale `Snapshot`, le identità vive e un `history_epoch` immutabile; primarie, ombre e riflessi vedono lo stesso stato. Gli snapshot precedenti restano utilizzabili dopo altri commit, anche nel fallback delle query a un tempo diverso. Il compositore legge l'epoch una volta per composizione e scarta la history quando cambia, compreso il ritorno a uno snapshot vecchio. È un reset globale conservativo: l'occupazione può cambiare ombre e riflessi su ricevitori non modificati. Aggiornamenti continui riducono quindi il riuso temporale; non è un'invalidazione selettiva per oggetto.

Il normale `SceneDocument::snapshot` conserva il proprio percorso. L'host senza playback usa `RuntimeSnapshot::untracked(inner)`, con epoch zero e senza allocazione delle identità; tutti gli hook di query delegano allo snapshot interno. I token runtime univoci vengono assegnati solo alla creazione e ai commit effettivi, senza contatori atomici per raggio.

### Timeline e launcher

[src/updates.rs](src/updates.rs) usa questa API per riprodurre un file eventi separato. `--updates` non cambia la versione della scena: il formato eventi ha `version: 1`, indipendente dalle scene v1 e v2. [dynamic-room-v2.json](assets/dynamic-room-v2.json) e [dynamic-updates-v1.json](assets/dynamic-updates-v1.json) forniscono un caso di inserimento, apertura tramite trasformazioni discrete, rimozione e riuso di un ID. Nessuna interpolazione o simulazione fisica è implicita.

```sh
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/dynamic-room-v2.json --updates clients/material-room/assets/dynamic-updates-v1.json --validate
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/dynamic-room-v2.json --updates clients/material-room/assets/dynamic-updates-v1.json --sequence 181 --camera all --profile detail --workers 8 --output evidence/my-dynamic-room
```

[Avvia-occupazione.cmd](../../Avvia-occupazione.cmd) ricompila sempre con `--locked --release`, avvia questi due file con profilo detail, worker automatici e output `evidence/r14/interactive`; inoltra gli argomenti aggiuntivi. Center resta il campionamento predefinito; `--sampling conservative9` o C selezionano l'altra policy.

Schema minimo di una sequenza riferita a un ID 3 inizialmente presente:

```json
{
  "version": 1,
  "events": [
    { "tick": 30, "operations": [
      { "op": "transform", "id": 3, "position": [1, 1, 0],
        "half_extents": [0.5, 1, 0.1], "rotation_y": 0.5 }
    ] },
    { "tick": 60, "operations": [{ "op": "remove", "id": 3 }] },
    { "tick": 90, "operations": [
      { "op": "insert", "object": {
        "id": 3, "name": "New panel lifetime", "shape": "box", "material": 33,
        "position": [0, 1, 0], "half_extents": [0.5, 1, 0.1]
      } }
    ] }
  ]
}
```

Il file UTF-8 ammette al massimo 1 MiB, 256 eventi e 128 operazioni per evento. I tick interi sono assoluti, a 30 tick/s, strettamente crescenti e nell'intervallo 0..2.592.000; unire nello stesso evento le operazioni dello stesso tick. Campi sconosciuti e chiavi duplicate sono respinti. `transform` richiede tutti i campi mostrati e usa gli stessi limiti del formato oggetto. I comandi del file risolvono l'ID nell'istanza viva al momento dell'evento; un'applicazione che conserva riferimenti fra commit deve usare gli handle dell'API Rust.

`Playback::new` prova l'intera sequenza su uno stato temporaneo prima di esporre il playback: anche un riferimento invalido in un evento futuro impedisce il caricamento. `--validate` controlla scena e sequenza senza creare GPU o finestra. `advance_to(tick)` applica in ordine tutti i batch dovuti, anche saltando più tick; il singolo batch è atomico, l'intero recupero di più eventi non lo è. Non si può tornare a un tick precedente; per ripartire occorre un nuovo playback. Dopo l'ultimo evento rimane l'occupazione finale, mentre eventuali movimenti authored non sostituiti continuano.

In headless ogni vista riparte dallo stesso documento e dalla stessa sequenza caricata una sola volta. `--time` applica prima del primo frame tutti gli eventi fino a `floor(time * 30)`; ogni frame successivo avanza di un tick. In finestra il tick segue il tempo di simulazione, con recupero degli eventi dovuti prima dello snapshot e pausa tramite Spazio. R ricarica atomicamente scena e sequenza e riparte da `--time`; senza `--updates`, R conserva invece il tempo corrente. Il limite di un giorno riguarda gli eventi e il tempo iniziale CLI: il playback può continuare oltre l'ultimo tick previsto.

Le transazioni vengono misurate separatamente da snapshot e composizione: `update_ms`, `commit_frame_update_ms` e `cpu_update_snapshot_and_compose_ms` nei report distinguono costo degli eventi e totale CPU. Prevalidazione e recupero iniziale a `--time` restano fuori dai timer dei frame. Non leggere il solo costo di composizione come costo completo degli aggiornamenti, né il tempo di submission/attesa come GPU time. Risultati della tranche e criteri di uscita appartengono al [report R14](../../DYNAMIC_OCCUPANCY.md), verificato con costi e limiti espliciti.

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

Gli ID sono assegnati dall'autore e non dalla posizione nell'array. In v2 un'uguaglianza esatta di distanza fra superfici è risolta dall'ID oggetto minore; riordinare materiali e oggetti conserva il risultato. La v1 mantiene invece l'ordine originale per non cambiare i vecchi output. Riutilizzare un ID per un contenuto diverso richiede un reset della history: il reload dell'host lo esegue sempre; il runtime R14 distingue la nuova generazione e cambia epoch al commit.

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

[tests/r11_composition_parity.rs](tests/r11_composition_parity.rs) confronta invece il compositore corrente in modalità predefinita con camera e compositore congelati al commit R11 `9866c35`, su dati v1/v2, tre viste, tempi, reset e uno/otto worker. [coverage_contract.rs](../../crates/ascii-render/tests/coverage_contract.rs) usa piani e strisce analitici indipendenti per verificare geometria sottile davanti a uno sfondo, occlusione completa, selezione per profondità, impronta materiale, disocclusioni e provenienza della history. Registra anche due limiti intenzionali: geometria ancora persa fra i campioni e supporto apparente allargato. Le copie congelate conservano i propri test originali: le loro esecuzioni ripetute non sono capacità aggiuntive.

Per R14, [dynamic_occupancy.rs](tests/dynamic_occupancy.rs) e i test di runtime/playback coprono transazioni, vecchi snapshot, identità e occupazione; [history_epoch.rs](../../crates/ascii-render/tests/history_epoch.rs) esercita l'invalidazione nel compositore. L'esempio [occupancy_audit.rs](examples/occupancy_audit.rs) è l'audit dedicato. Test workspace/release, audit e runtime nelle tre visuali sono passati; misure complete e limiti sono nel report R14.

## Evidenze

Ogni esecuzione headless salva `run.json`, la **scena validata effettivamente caricata**, versione dati, camere iniziali, velocità d'orbita, percorso, campionamento, griglia, worker, contatori temporali e CSV. Le distribuzioni CPU separano snapshot, composizione e totale; la submission/attesa GPU è separata. Dodici warmup esclusi; PNG e audit fuori dai timer. In `--cpu-only` le capacità GPU e la distribuzione submission/attesa sono `null`; il CSV usa zero come segnaposto per l'attesa, non come misura GPU. L'audit incluso nel normale `run.json` usa solo raggi centrali e non misura la copertura del percorso a nove campioni: per questo serve `coverage_audit`.

La finestra salva `window-run.json` nella directory `--output`: intervalli reali fra callback, CPU snapshot/composizione e durata CPU di acquire/submit/present, con i primi dodici frame esclusi e conteggio dei cambi di configurazione. Per una misura confrontabile lasciare invariati vista e controlli durante l'esecuzione. Gli intervalli includono schedulazione e lavoro precedente; acquire/submit/present e attesa headless non sono timestamp GPU o misure di scansione del display. Catture e scritture possono influenzare la schedulazione. Statico identico non prova da solo assenza di sfarfallio in movimento, e 30 tick simulati/s non sono una misura di 30 FPS. Risultati storici R6 in [REUSE.md](../../REUSE.md); stato corrente e limiti verificati in [PROJECT_STATUS.md](../../PROJECT_STATUS.md), criteri della tranche in [ENGINE_PATH.md](../../ENGINE_PATH.md).
