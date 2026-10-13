# R3 — portabilità, budget e snapshot

Il processore della regia è una macchina di misura, non un requisito del motore. Questa tranche rende esplicita la gestione delle risorse e prepara una prova riproducibile su altri computer. Non certifica hardware che non è stato eseguito.

## Implementato

- Policy CPU in `src/budget.rs`, senza riconoscimento del modello del processore. La capacità iniziale arriva da `std::thread::available_parallelism`; `--workers auto` rispetta il tetto del profilo e lascia, quando possibile, un'unità fuori dal pool. Questo non riserva un core fisico: la distribuzione dei thread resta al sistema operativo. Un numero esplicito è ammesso entro la capacità stimata e il limite applicativo 64; `1` usa il percorso seriale.
- Profili fissi economy/balanced/detail: rispettivamente 120×68, 160×90, 240×135 caratteri e tetti automatici 2/8/16. Sul PC attuale la risoluzione automatica è 2/8/15 worker. Gli override dimensionali prevalgono sul profilo indipendentemente dall'ordine degli argomenti. Nessuna modifica nascosta della qualità durante il movimento.
- `ActorSnapshot` prepara le 15 parti della figura una volta; `WorldSnapshot` conserva posa, luce, tempo e stage. Le query parallele leggono lo stesso stato immutabile per immagine, ombre e riflessi. La matematica delle intersezioni, i glifi e il solver del tessuto restano quelli di R2. `Scene` resta sostituibile da un altro utilizzatore; il wrapper storico continua a funzionare.
- Selezione esplicita del backend e della preferenza energetica GPU. Auto rispetta `WGPU_BACKEND`; un backend esplicito non viene sostituito silenziosamente. Controlli di capacità/formati e limiti di buffer, texture e readback prima dell'allocazione, anche per il resize. Nessuna feature GPU opzionale richiesta per il renderer attuale; niente compute o timestamp query attivati.
- `runtime.json` accompagna le prove headless con budget risolto, OS/architettura, adapter/driver/backend, requisiti e limiti effettivi. `--diagnostics` inizializza il percorso offscreen e salva questo file; non collauda la superficie della finestra. Se l'inizializzazione fallisce prima, l'errore è in console e il JSON non viene prodotto.

Il nucleo non richiede API Ryzen o un conteggio fisso di core. La build controllata usa il normale target Rust `x86_64-pc-windows-msvc`, senza `target-cpu=native` o flag CPU aggiuntivi trovati nel progetto, nella configurazione Cargo utente o nell'ambiente della prova. È comunque un eseguibile Windows x64: non è un unico file valido per ogni sistema operativo o architettura.

## Cosa significa adattarsi

Compatibilità significa poter creare un dispositivo e usare le funzioni necessarie. Scalabilità significa poter ridurre o aumentare il lavoro. Fluidità significa rispettare il tempo del fotogramma nelle condizioni reali di quel computer. Un numero di core disponibile risponde solo a una parte della seconda domanda: non misura frequenza, cache, memoria, temperatura, processori ibridi o carico degli altri programmi.

Perciò i profili sono punti di partenza espliciti, non un benchmark automatico. Non c'è ancora una calibrazione iniziale della macchina, una qualità dinamica, uno scheduler per core eterogenei o una garanzia di 60 fps. La stima runtime può essere imperfetta sotto affinità, job Windows o virtualizzazione; il controllo manuale resta disponibile. [Contratto e limiti di available_parallelism](https://doc.rust-lang.org/std/thread/fn.available_parallelism.html).

wgpu astrae più API grafiche, ma il renderer deve comunque verificare le proprie esigenze. Qui servono lettura di storage buffer nel fragment shader, atlas R8Unorm campionabile e caricabile, cattura Rgba8UnormSrgb renderizzabile e leggibile e, a finestra, superficie sRGB con present FIFO. GL/WebGL non sono dati per compatibili. Le capacità dichiarate dal driver non garantiscono che ogni allocazione riesca. [API wgpu 26](https://docs.rs/wgpu/26.0.1/wgpu/).

## Matrice effettivamente eseguita

Host: Windows x64, AMD Ryzen 7 3800X, 8 core fisici/16 thread logici; stima runtime 16. Rust 1.97.1 MSVC, release, dipendenze bloccate. Questo resta **un solo host fisico**.

| Percorso | Prova | Esito |
| --- | --- | --- |
| Radeon RX 6700 XT, Vulkan, driver AMD 26.8.1 | Quattro stage × tre visuali, catture, scena congelata, animazione e pan | PASS |
| Stessa Radeon, DirectX 12, driver 32.0.21045.5002 | Stessi 12 casi; validazione indiretta wgpu attiva | PASS dopo correzione del budget descrittori |
| Vulkan e DirectX 12 a finestra | Due smoke con chiusura automatica dopo 2 s richiesti | PASS; non un benchmark FPS/present |
| Profili economy e detail, backend auto | Cattura completa Cloth/terza, dimensioni e worker risolti | PASS |
| Microsoft Basic Render Driver, DX12, tipo Cpu | Fallback esplicito, Cloth/terza 80×45 a 640×360 | PASS; renderer software, non una seconda GPU fisica |
| Metal richiesto sulla build Windows | Errore esplicito, nessuna sostituzione | PASS come rifiuto atteso |
| Altre CPU AMD/Intel, ARM, GPU NVIDIA/Intel/integrate | Nessuna esecuzione su tali dispositivi | Da verificare |
| Linux/macOS, Metal su macOS, GL, browser, mobile | Nessuna build/esecuzione validata in R3 | Da verificare; nessuna promessa di supporto |

Comandi, exit code e log: `evidence/r3/runtime-checks.json`; capacità nei sottopercorsi `runtime.json`. Il fallback software usa ancora questa CPU e non dimostra prestazioni di una GPU integrata.

La prova DX12 ha scoperto un problema reale del primo budget minimo: tre descrittori bastavano ai nostri binding, ma wgpu 26 ne occupa uno persistente per la validazione interna delle chiamate indirette. Su DX12 il limite dimensiona l'heap globale. Con tre slot il nostro bind group falliva con OutOfMemory; la diagnosi sul vecchio binario con il solo flag di validazione disattivato ha isolato la causa. La correzione riserva **3 + 1** e mantiene la validazione attiva; sono stati aggiunti scope d'errore per inizializzazione/allocazione. Il flag diagnostico non fa parte della configurazione consegnata. Evidenze: `dx12-failure.txt`, `dx12-isolation/`, `dx12/runtime.json`. La riserva è documentata per la dipendenza bloccata wgpu 26, da riesaminare prima di un aggiornamento del backend.

## Parità e misure

La suite completa è passata con 56 esecuzioni di test, comprese copie dei test storici importate come oracolo; non sono 56 funzionalità indipendenti. Dopo la correzione DX12 sono ripassati i sei test GPU, clippy e la build. Fmt verificato; il solo avviso di compilazione riguarda hard link non disponibili nella cache incrementale, sostituiti da copie.

Il confronto snapshot/R2 verifica byte delle celle e statistiche temporali: tre visuali, quattro stage, tre budget worker e sei tick con movimento camera e reset della storia, 216 combinazioni a 47×29 celle. Il test attore verifica anche hit bit per bit da più punti di osservazione. Le catture GPU a 160×90 sono confrontate con quelle R2; conteggio e hash in [summary](evidence/r3/summary.md). Resta la stessa immagine del laboratorio precedente, non un miglioramento artistico del mantello o delle superfici.

L'esperimento CPU confronta il codice congelato world/actor R2 con lo snapshot, ricompilati nello stesso eseguibile con lo stesso compositore. Cloth, tre camere, 240×135 celle, worker 1 e 8, tre ripetizioni, 12 warm-up e 120 campioni per variante. Include creazione snapshot, composizione e rilascio del buffer; esclude GPU, present e avvio del pool.

Il primo protocollo a blocchi (`evidence/r3/cpu/`) ha mostrato forti oscillazioni del desktop. È conservato integralmente. La seconda prova (`cpu-paired/`) alterna le due varianti all'interno di ogni frame, invertendo l'ordine a ogni frame/ripetizione, con due storie/pool privati e un solo compositore attivo alla volta. Riduce la deriva temporale, ma non rende il desktop un ambiente isolato. Non sono state eseguite build o altre prove del laboratorio in parallelo.

Risultati completi in [summary.md](evidence/r3/summary.md) e [summary.json](evidence/r3/summary.json). Le mediane dei percentili delle tre ripetizioni non sono percentili dell'insieme dei campioni; gli intervalli min/max descrivono la dispersione, non intervalli di confidenza. Non confrontare questi numeri direttamente con una vecchia sessione R2 per attribuire tutto lo scarto allo snapshot. Nessun numero è un FPS finestra o una certificazione su CPU più lenta.

| Worker | Vista | CPU p50 riferimento R2 | CPU p50 snapshot |
| --- | --- | --- | --- |
| 1 | Prima | 35,56 ms | 30,03 ms |
| 1 | Terza | 29,49 ms | 24,83 ms |
| 1 | Dall'alto | 33,39 ms | 28,08 ms |
| 8 | Prima | 5,66 ms | 5,07 ms |
| 8 | Terza | 4,95 ms | 4,53 ms |
| 8 | Dall'alto | 5,37 ms | 4,83 ms |

In questa prova abbinata, riduzione del tempo CPU mediano di circa 16% in seriale e 8–10% con otto worker. Tutte le 18 coppie camera/budget/ripetizione hanno p50 minore con snapshot; non tutte hanno p95 minore. Esempio: dall'alto/8 worker/ripetizione 2, p95 7,39→7,62 ms. Nella prima ripetizione seriale si osservano p95 oltre 100 ms: non attribuiti a una causa misurata, non rimossi dal dataset. L'ottimizzazione riduce lavoro ripetuto ma non risolve ogni picco del desktop.

Verifica catture conclusa: **24/24 PNG identiche byte per byte** alla baseline R2/Vulkan, comprese tutte quelle DX12. Baseline R2: 13/13 hash conservati per sorgenti copiati, manifest Cargo, esempio ed eseguibile. Ispezionata anche la cattura economy: minore densità esplicita, stessa scena e stile; nessuna nuova validazione percettiva continua dichiarata.

Un conteggio separato e non temporizzato a t=1 mostra 32.400 raggi primari per ogni vista e rispettivamente 23.382/18.995/26.369 query secondarie in prima/terza/dall'alto. Sono chiamate `Scene::trace`, non test di intersezione singoli. Lo snapshot riduce la preparazione ripetuta dello stato, non il numero di raggi o primitive visitate: resta da profilare dove conviene accelerare le query.

## Riproduzione e passo successivo

Dal laboratorio, con build release aggiornata:

```powershell
cargo test --locked --release --all-targets
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release --bins --examples
.\scripts\verify-windows-backends.ps1
.\target\release\examples\snapshot_bench.exe --paired
python .\scripts\summarize-portability.py
```

Lo script di verifica è specifico per questa matrice Windows: alcuni adapter/backend possono mancare su un altro PC. La sintesi locale usa le catture e la baseline R2 conservate. Per una prima prova su un altro PC Windows compatibile, bastano eseguibile release e questi comandi, con un nuovo output:

```powershell
.\ascii-engine-lab.exe --diagnostics --output evidence\other-machine
.\ascii-engine-lab.exe --benchmark-case --profile economy --workers auto --frames 120 --output evidence\other-machine
```

Conservare runtime.json, benchmark.json e CSV, annotando modello CPU, sistema e condizioni di carico. Non serve aprire un'altra chat per raccogliere questo pacchetto. Una prova con 1/2 worker sul Ryzen non sostituisce quel collaudo.

Prossimo lavoro tecnico: campionare il costo delle intersezioni e dei raggi secondari, quindi scegliere una sola accelerazione misurabile. Il margine va destinato anche a silhouette, materiali e qualità temporale. SIMD specializzato o GPU compute richiederanno un percorso generale, verifica capacità e confronto equivalente; non sono ancora implementati. Rimangono procedurali il tessuto e la luce, planare il riflesso d'acqua; nessun editor o gameplay aggiunto.

Godot e i suoi asset restano storici. Baseline R2, sorgenti e hash sono preservati in `evidence/r3/baseline/`; nessun repository nuovo, commit o pubblicazione.
