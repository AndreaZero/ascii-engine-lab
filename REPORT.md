# R1 — rapporto della prima prova Rust + wgpu

Questo documento conserva le prove R1 seriali. La build corrente include il pool CPU R2, descritto con misure ripetute e limiti in [PERFORMANCE.md](PERFORMANCE.md). La baseline R1 con sorgenti/eseguibile è preservata in evidence/r2/baseline; il manifest originale resta invariato.

13 ottobre 2026. Stato: laboratorio standalone implementato ed eseguito; direzione artistica finale e motore di produzione non validati. Unica regia della chat confermata, gameplay BLACKWATER sospeso, Godot preservato.

## Implementato e verificato

- Libreria Rust con contratto `Scene`, `Ray`, `Hit`, `GlyphCell`; fixture separata dal compositore wgpu. Nessun accesso al runtime Godot.
- Uscita diretta a caratteri da query analitiche. La GPU riceve un buffer di glifi e colori, non un render a mesh del mondo.
- Quattro stage: core, luce puntuale mobile/ombre, acqua con query riflessa, figura con tessuto localmente deformato.
- Tre visuali condividono lo stesso mondo/tick: prospettiva a quota occhi, esterna orbitabile, ortografica alta. Sono osservatori di laboratorio, non controller gameplay completi.
- Build release riuscita; cargo fmt e clippy all-targets senza diagnostiche di codice; **13 test passati**. Il compilatore segnala soltanto il fallback da hardlink a copia della cache incrementale locale.
- wgpu realmente eseguito offscreen con catture PNG. Smoke test a finestra completato: **215 frame in circa 3 secondi**; il contatore include disegni richiesti e non è una misura affidabile di present/FPS. Chiusura automatica senza errori restituiti.
- Revisione indipendente: corretti contaminazione della storia fra celle, statistiche con ID ambigui e redraw CPU durante sospensione/minimizzazione. Test di immagine congelata aggiunto e superato. Resize/minimizzazione hanno gestione nel codice, ma non è stato eseguito un test UI manuale completo.

## Prestazioni misurate

Macchina: AMD Ryzen 7 3800X, AMD Radeon RX 6700 XT; Vulkan, driver AMD 26.8.1. Windows, Rust 1.97.1, release con thin LTO. Una sessione locale desktop per configurazione: 120 frame misurati dopo 12 di warm-up, tempo simulato a 1/60 s; nessun isolamento dei carichi di sistema. I dati non costituiscono un benchmark hardware universale.

Stage completo `cloth`, tutti i tempi in millisecondi:

| Output / griglia | Camera | CPU p50 / p95 | Upload+submit+wait p95 | Frame offscreen p50 / p95 / p99 |
| --- | --- | --- | --- | --- |
| 1280×720 / 160×90 | first | 14.71 / 16.69 | 1.51 | 15.46 / 17.56 / 18.57 |
| 1280×720 / 160×90 | third | 11.38 / 14.48 | 1.72 | 12.21 / 15.25 / 17.23 |
| 1280×720 / 160×90 | overhead | 14.15 / 17.69 | 1.98 | 14.99 / 18.37 / 27.99 |
| 1920×1080 / 240×135 | first | 32.00 / 36.63 | 1.87 | 32.96 / 37.47 / 38.18 |
| 1920×1080 / 240×135 | third | 27.04 / 30.24 | 2.01 | 28.09 / 31.33 / 35.44 |
| 1920×1080 / 240×135 | overhead | 29.35 / 34.22 | 1.76 | 30.33 / 35.42 / 36.96 |

Il primo caso ha 14.400 celle, il secondo 32.400: cambiano sia query sia pixel. Il costo prevalente è CPU. Il budget esplorativo di 16,7 ms non è rispettato al p95 da tutte le viste a 720p ed è superato da tutte le viste complete a 1080p. Non è stata ancora eseguita una prova a output diverso e identica griglia per isolare il costo dei pixel.

`submit_and_wait` comprende upload, driver, lavoro GPU e sincronizzazione. Non ci sono timestamp GPU: il tempo GPU puro resta **non misurato**. Frame offscreen sincronizzato non equivale a frame presentato in finestra. Nessun benchmark equivalente Godot è stato svolto; nessuna superiorità di Rust è dimostrata.

Dati completi per tutti i 24 casi (4 stage × 3 camere × 2 configurazioni): [720p](evidence/720p/benchmark.json), [1080p](evidence/1080p/benchmark.json), più CSV per-frame nelle rispettive cartelle. La versione del binario e gli hash del sorgente sono nel [manifest delle prove](evidence/manifest.json).

## Stabilità misurata

**Zero celle differenti** per tutti i 24 casi sia fra composizioni da zero sia fra composizioni consecutive a scena/camera/tempo congelati. Non basta a garantire stabilità in movimento.

Pan con mondo congelato: 30 campioni con incremento angolare 0,004 rad. Si confrontano solo celle nella stessa posizione schermo e con lo stesso ID locale quantizzato; nessuna riproiezione completa. Le celle non abbinate, il cielo e le disocclusioni non entrano nel rapporto di cambi. La copertura sotto è rispetto all'intera griglia su 29 transizioni. Stage cloth a 720p:

| Camera | Glifi cambiati fra celle abbinate nel pan | Copertura confrontata | Cambi con animazione/luce (celle abbinate) |
| --- | --- | --- | --- |
| first | 3.04% | 35.3% | 0.201% |
| third | 5.17% | 41.6% | 0.036% |
| overhead | 6.73% | 54.1% | 0.044% |

Questi cambi includono variazioni lecite del campionamento/della forma e non sono tutti sfarfallio. I valori indicano dove analizzare i filmati, non certificano comfort visivo. La memoria conserva un glifo solo nella stessa cella con ID invariato e variazione luminosa limitata; nessun trasferimento indiscriminato fra campioni dello stesso bin.

## Evidenza visiva e limiti osservati

Catture GPU per ogni stage e camera nelle cartelle delle misure. [Clip di laboratorio](evidence/laboratory-demo.mp4): prima persona, terza, alto, retro, tre secondi ciascuna a passo simulato 1/30 s. Sono fotogrammi offscreen codificati in video, non una registrazione del frame pacing della finestra.

Ispezione delle catture: arco e riflesso sono riconoscibili, materiali hanno alfabeti diversi, nella vista a quota occhi si distinguono testa/torso/stivali. Braccia e mani si separano poco, il tessuto ha una trama a chevron troppo regolare, i ciuffi sembrano masse ellissoidali. Nel percorso posteriore la figura quasi scompare col movimento della luce: il buio esiste, ma la leggibilità va progettata meglio. La resa è essenziale e lontana dall'ambizione delle reference.

Camera e luce in movimento non isolano la deformazione. Per questo `examples/cloth_study.rs` usa lo stesso motore con una `Scene` della sola figura, luce e camera ferme: una prova di riutilizzo minimo e di animazione locale. La [clip dedicata](evidence/cloth-isolated.mp4), generata e controllata su fotogrammi a 0 e 1,5 s, mostra variazioni locali dell’orlo mantenendo la zona delle spalle pressoché ancorata. La trama resta troppo regolare. Va valutata insieme ai test degli ancoraggi e delle coordinate materiali. Il campionamento di alcuni fotogrammi non equivale a una validazione percettiva continua.

## Approssimato e mancante

- **Tessuto:** onde locali analitiche, non fisica; niente auto-collisione. Il solver d'intersezione limitato può perdere contatti radenti. Figura da ellissoidi, nessuna locomozione giocabile o nemico completo.
- **Luce/materiali:** termine ambientale artistico e luce puntuale con ombre dure; niente illuminazione globale e risposta speculare per materiale ancora limitata.
- **Acqua:** piano con normale perturbata, un rimbalzo di query dallo stesso tick; niente volume fluido, rifrazione o simulazione delle onde geometriche.
- **Campionamento:** un raggio per cella, nessun dettaglio adattivo o antialiasing temporale; rischio di perdita dei dettagli e popping in movimento. Il font bitmap è un punto di partenza, non lo stile definitivo.
- **Camere:** transizioni a taglio, yaw/orbita e traslazione libera; nessuna collisione camera, mani/player posseduto o controllo combat. Il tempo della scena sopravvive al cambio vista.
- **Validazione:** prestazioni brevi su una sola macchina; nessun GPU timestamp, test multipiattaforma o confronto equivalente Godot. Clip offline non misura latenza input o pacing del display.
- **Prodotto:** nessun editor, asset pipeline, API stabile, networking o espansione gameplay.

## Tranche successiva proposta, non avviata

1. Accelerare le query CPU con broad phase/struttura spaziale e rimuovere calcoli ripetuti; confrontare dati identici e profili prima/dopo. Valutare compute soltanto in base al costo residuo.
2. Migliorare silhouette e grammatica del tessuto e dei materiali, distinguendo pieghe/cuciture; conservare campioni e ridurre cambi non motivati. Il buio non deve cancellare ogni indizio della figura.
3. Ampliare prove controllate di pan/zoom e riflesso; confrontare a griglia costante; valutare un produttore a campioni persistenti contro le query quando esiste una baseline visiva stabile.

Le ownership devono essere nuovamente assegnate dalla regia prima di scrivere. Non serve aprire altre chat ora: gli stessi subagenti possono ricevere una tranche circoscritta. Per filoni persistenti aprire manualmente una chat nello stesso workspace e usare i prompt aggiornati in `../ascii-game/docs/CHAT_STARTERS.md`.
