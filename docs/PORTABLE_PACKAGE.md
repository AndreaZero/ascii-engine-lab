# Pacchetto Windows: Material Room e laboratorio

Il pacchetto contiene i due client, il programma `author.exe` e un progetto
iniziale completo in `projects/starter.project.json`. Si può estrarre e spostare
la cartella, anche in un percorso con spazi o caratteri Unicode. Non servono
checkout, Cargo, Rust, Python o Node per aprire il progetto o eseguire il probe.
I font e gli shader necessari sono incorporati negli eseguibili.

Il target è **Windows x64 MSVC**. Servono il runtime Microsoft Visual C++ x64
(`VCRUNTIME140.dll`) e Universal CRT; questa distribuzione non li include e non
li installa. Serve inoltre un driver con un backend compatibile con i requisiti
wgpu del renderer. Un errore di caricamento DLL precede l'applicazione e può
impedire la creazione del suo JSON: il probe registra anche questo caso.
Una DLL presente sul disco da sola non prova che il caricamento riesca.

R15 non cambia il collegamento CRT in statico. Il pacchetto non contiene un
installer di driver, non modifica il sistema e non scarica dipendenze.
Le licenze del progetto e delle dipendenze sono in `LICENSE` e
`third-party-licenses/`, con inventario e testi; alcuni archivi upstream privi
di un file licenza separato sono conservati interi con i loro avvisi incorporati.

## Aprire e modificare un progetto

`Start-MaterialRoom.cmd` crea una copia del progetto iniziale in
`work/room.project.json` al primo avvio e riapre quella copia agli avvii successivi,
senza sovrascriverla; `Start-Laboratory.cmd` apre
il cortile indipendente. I launcher risolvono i percorsi dalla propria cartella.
Material Room senza `--project` o `--scene` usa il campione v1 incorporato,
senza leggere il checkout della macchina che ha compilato il programma.
Il progetto R15 contiene scena v2, timeline v1 e controlli delle tre camere:
nessun rimando a scene o asset esterni.

Per conservare verificabile il contenuto distribuito, creare il proprio progetto
in `work/`, lasciando invariato `projects/starter.project.json`. Da PowerShell
nella cartella estratta:

```powershell
New-Item -ItemType Directory -Force work
.\author.exe new "work\La mia stanza.project.json"
.\author.exe inspect "work\La mia stanza.project.json"
.\ascii-material-room.exe --project "work\La mia stanza.project.json"
```

In finestra: `1/2/3` selezionano le visuali; `Q/E`, `WASD`, `Z/X` muovono
la camera; `Tab` seleziona un oggetto; frecce e `PgUp/PgDown` lo spostano;
`Ctrl+D` duplica, `Delete` rimuove, `Ctrl+S` salva. `F5` riavvia l'anteprima.
La chiusura non salva automaticamente: con modifiche pendenti richiede
`Ctrl+S` per salvare oppure `Shift+Esc` per scartare e uscire.
La camera salvata appartiene al client,
con stato distinto per ciascuna vista. Gli edit si applicano allo stato iniziale
autoriale; non salvano automaticamente il risultato trascorso della timeline.

Gli edit JSON usano `author apply FILE PATCHFILE`. Per esempio, un file UTF-8
`work/cambia-oggetto.json`:

```json
{"version":1,"operations":[
  {"op":"replace","path":"/scene/objects/1/position","value":[1,1,0]}
]}
```

```powershell
.\author.exe apply "work\La mia stanza.project.json" work\cambia-oggetto.json
.\ascii-material-room.exe --project "work\La mia stanza.project.json" --validate
```

L'indice array è zero-based: controllare `array_index` e ID con `inspect` prima
di applicare la patch. Sono ammessi `add/remove/replace` nei sottoinsiemi
`/scene`, `/updates`, `/camera_controls`, al massimo 128 operazioni. Il candidato
e l'intera timeline vengono validati prima di salvare. Errori e conflitti non
autorizzano a sovrascrivere il documento; leggere il messaggio e riaprire il file.
Il salvataggio usa un lock cooperativo e verifica i byte letti: editor esterni
che non rispettano il lock possono ancora concorrere con l'ultima verifica.
Non rimuovere automaticamente lock esistenti. `author --help` elenca anche
importazione, duplicazione, trasformazione, materiale e copia `save-as`.

## Raccogliere una prova su Windows

Il probe richiede PowerShell a 64 bit, 5.1 o successivo. Non esegue build, non
scarica runtime e non modifica execution policy. Se una policy locale impedisce
lo script, usare le normali regole di autorizzazione del proprio computer.

```powershell
.\probe-material-room.ps1 -Backend dx12 -MachineRole development-host
.\probe-material-room.ps1 -Backend vulkan -Windows -MachineRole other-physical-host -LoadNotes "Applicazioni di misura chiuse"
```

`Probe.cmd` richiama lo stesso script. `-MachineRole` è una dichiarazione
dell'operatore, non un rilevamento automatico dell'identità hardware; usare
`other-physical-host` solo su un altro computer fisico. `-MachineLabel` è una
nota facoltativa: non inserire seriali o altri dati privati. `-OutputDirectory`
accetta solo una destinazione nuova. In assenza di override, i risultati sono
in una nuova sottocartella di `results/`.

La prova verifica gli hash prima di avviare i programmi, legge il progetto con
`author`, valida progetto e scena v1 incorporata, poi cattura prima persona,
terza persona e alto con entrambi i client. Material Room produce due PNG per
vista (primo e ultimo dei tre frame); il laboratorio un PNG Cloth per vista.
Ogni processo parte da una directory di lavoro vuota creata nel risultato,
quindi non dipende dalla directory corrente del terminale. I percorsi del
progetto e degli eseguibili sono risolti dentro il pacchetto.

`-Windows` aggiunge sei avvii a finestra, tre per client, chiusi dopo i secondi
richiesti. Il client produce `window-run.json`; il laboratorio al momento
conserva solo log ed exit code per la finestra. Questo è uno smoke test di avvio
e chiusura: osservazione visiva e uso interattivo vanno riportati separatamente.
Un processo che supera il timeout viene terminato e registrato come fallimento.

Backend, profilo e worker sono fissati per l'intera esecuzione: default DX12,
balanced, auto. Si possono cambiare esplicitamente con `-Backend`, `-Profile`,
`-Workers`; non c'è ripiego silenzioso dopo un errore. Auto worker è un budget,
non una misura della velocità del processore. I tre frame del probe non sono
un benchmark né una promessa FPS. Nessun valore di attesa è chiamato GPU time.

`probe.json` conserva OS/build, modello CPU, GPU/driver dichiarati da Windows,
capabilities dell'adapter effettivamente scelto nei JSON nativi, configurazione,
hash, exit code, tempi e fase di fallimento. Se il driver fallisce prima di
scrivere JSON, rimangono stdout/stderr e il fallimento del caso. Fallimenti di
inventario host sono riportati; gli altri casi indipendenti proseguono dopo un
errore runtime. Il processo termina con codice nonzero se un caso richiesto
fallisce. Conservare tutta la cartella, inclusi PNG e log, anche in caso di errore.
I log/JSON nativi possono includere percorsi locali: rivederli prima di pubblicare.

Una copia in un'altra cartella, una VM, un altro backend sulla stessa GPU o meno
worker non equivalgono a un secondo computer fisico. Alla preparazione R15 non
è disponibile una seconda macchina verificata. Il report tecnico di tranche
deve distinguere prove di trasferimento sullo stesso host da risultati ricevuti
da un altro host. Su un'altra GPU non si presume identità dei PNG senza una
verifica; si conservano immagini, metadati ed eventuali differenze.

## Assemblaggio riservato al checkout di sviluppo

La regia compila e verifica preventivamente i tre eseguibili release e prepara
le licenze da metadati Cargo bloccati. Lo script non compila e non consulta la
rete; richiede una destinazione nuova e lascia gli output incompleti per diagnosi.

```powershell
cargo build --locked --release --workspace --bins --examples
New-Item -ItemType Directory -Force evidence\r15
cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc | Out-File -Encoding utf8 evidence\r15\cargo-metadata.json
python .\scripts\collect-package-licenses.py evidence\r15\cargo-metadata.json evidence\r15\package-licenses
.\scripts\pack-material-room.ps1 -OutputDirectory "evidence\r15\pacchetto Windows" -LicenseDir evidence\r15\package-licenses
python .\scripts\check-portable-package.py "evidence\r15\pacchetto Windows"
python .\scripts\check-portable-package.py "evidence\r15\pacchetto Windows.zip"
```

`-ReleaseDir` può indicare un'altra directory già compilata. Il comando `python`
qui è solo il verificatore indipendente di sviluppo, stdlib: scegliere un runtime
Python 3.12 o successivo funzionante, non l'alias WindowsApps. Non è richiesto
nel pacchetto. Anche la directory del collector deve essere nuova: per una
seconda preparazione scegliere un altro nome e passarlo con `-LicenseDir`.
Il manifest registra revisione/stato dei sorgenti, hash e dimensione di ogni file;
questo non dimostra da solo che binari forniti e commit corrispondano. La regia
conserva i log della build e dei controlli corrispondenti. La ZIP ha un hash esterno;
gli hash verificano integrità, non sono firme digitali. I binari possono ancora
contenere stringhe diagnostiche del compilatore con percorsi dei sorgenti: non
sono dipendenze di caricamento runtime.

Il verificatore indipendente controlla percorsi relativi, collisioni Windows,
hash/dimensioni, notice, struttura del progetto e architettura/import PE. Rifiuta
file estranei al payload, salvo le directory locali `results/` e `work/` nella
verifica di una cartella. Non valida esecuzione, qualità artistica, prestazioni,
driver o seconda macchina. La prova reale è il passo successivo all'assemblaggio.
