# R8 — leggibilità del bagnato

13 ottobre 2026. [Report, limiti e riproduzione](../../WET_READABILITY.md).

- Variante wet_glyph del solo contenuto pietra; intervallo [0,025; 0,13), stesse regioni filtrate. Materiale, RGB di cella, luce, geometria, font e renderer invariati. Crepe e buio profondo conservati. WetR7 resta selezionabile.
- 121 esecuzioni test PASS (incluse copie storiche), fmt/clippy/release PASS. 81 confronti esatti di composizioni per colore/ID/flag/blank. 8/8 hash baseline preservati, Godot invariato.
- 1.128 PNG identiche a R7; 30 coppie statiche Vulkan/DX12 identiche. Matrice 72 casi / 9.774 frame; 48 controlli mirati / 8.640 frame. RGB/impronte di identità, camera, luce, visibilità e blank abbinati. Nessun taglio finale, statico senza cambi.
- Celle rade non vuote della pietra: prima 16,61→8,16%, terza 26,76→12,05%, alto 35,18→10,50%. Quote vuote invariate; non una misura di bellezza. Proxy del segnale medio +3,9 / +8,4 / +16,9%, non luminanza percettiva. Nessun miglioramento dello speculare rivendicato.
- Windows x64, Ryzen 7 3800X identificato in precedenza, RX 6700 XT rilevata per run, Vulkan 26.8.1/DX12. Tre ripetizioni CPU-only a 240×135: R8 p50 mediani 3,92 / 4,21 / 5,00 ms con 8 worker; peggiori p95 9,43 / 10,07 / 11,84 ms. Nessuna accelerazione dimostrata.
- **Prestazioni non uniformi:** prima matrice 30/72 casi oltre 16,7 ms, inclusa R7 invariata. R8 p95 dinamici fino a 47,24 ms economy e 38,24 ms balanced; registrazione completa fino a 20,47 ms. Prime misure conservate. Formattazione sovrapposta all'inizio della serie e altre cause non isolate: nessuna attribuzione certa. Controllo mirato successivo senza altri nostri comandi di calcolo: 48/48 entro budget, R8 peggiori p95 2,52 / 4,15 / 14,67 ms per griglia. Non certifica FPS stabili.
- Working set CPU-only 17,24–18,13 MiB; prima matrice 246,36–258,26 MiB; registrazione completa 258,53 MiB. Non VRAM. Submission+attesa detail nel controllo 0,94–0,97 ms p50; non GPU time.
- 1.620 PNG GPU → due filmati 18 s, 2560×1080, 30 Hz simulati, 540 frame ciascuno. Crop dei soli margini controllato ed etichette; decodifica completa PASS. Media e registrazioni grezze restano locali.

Resa ancora stilizzata e ombre profonde ancora nere; grana recuperata nelle mezze ombre. La copertura font non misura i pixel GPU a ogni scala. I contatori di cambi glifo non certificano fluidità o assenza di sfarfallio. Prossima generalizzazione: materiale distinto con un provino proprio, preservando R7/R8.
