# Authoring pratico di Material Room — R15

Un progetto raccoglie la scena iniziale, la sequenza degli aggiornamenti e le
tre camere in un solo JSON. Il comando `author` modifica e valida questi dati;
la finestra di Material Room permette di provarli e fare piccoli spostamenti.
Il formato usa le forme e i materiali già supportati dal client. Non aggiunge
un editor universale, geometria a mesh, fisica o gameplay.

Questa guida descrive il contratto R15. Risultati verificati e limiti appartengono a
[PROJECT_STATUS](../../PROJECT_STATUS.md).

## Creare e aprire un progetto

Dalla radice del repository `ascii-engine-lab`, compilare entrambi i programmi:

```sh
cargo build --locked --release -p ascii-material-room --bins
```

Su Windows PowerShell, sempre dalla radice del repository:

```powershell
.\target\release\author.exe new my-room.json
.\target\release\author.exe inspect my-room.json
.\target\release\ascii-material-room.exe --project my-room.json --validate
.\target\release\ascii-material-room.exe --project my-room.json
```

`new` crea il campione v2 con una timeline vuota. La directory di destinazione
deve esistere; un file già presente non viene sovrascritto. `inspect` mostra ID,
posizioni negli array, materiali, oggetti, eventi e controlli delle camere.
`--validate` controlla anche tutti gli eventi futuri senza aprire GPU o finestra.

Il progetto incorpora i dati: non conserva riferimenti ai file sorgente.
`--project` è incompatibile con `--scene` e `--updates`. I vecchi ingressi
separati rimangono disponibili; senza nessuno di questi argomenti il programma
usa il campione v1 incorporato nell'eseguibile.

Per raccogliere una scena v2 e una timeline v1 esistenti:

```powershell
.\target\release\author.exe import my-dynamic-room.json clients/material-room/assets/dynamic-room-v2.json clients/material-room/assets/dynamic-updates-v1.json
.\target\release\ascii-material-room.exe --project my-dynamic-room.json --validate
```

L'importazione non migra scene v1. Il formato delle scene e la migrazione
esplicita a v2 sono descritti nel [README](README.md#json-v2-superfici-conduttori-ambiente-e-camere).

## Modificare dalla finestra

| Tasto | Effetto con `--project` |
| --- | --- |
| **1 / 2 / 3** | Prima persona, terza persona, vista dall'alto; ogni vista conserva i propri controlli |
| **Q / E** | Ruota la camera intorno all'asse verticale |
| **W / S**, **A / D** | Avanti/indietro e spostamento laterale della camera sul piano orizzontale |
| **Z / X** | Abbassa/alza la camera |
| **Tab** | Passa al prossimo oggetto della scena iniziale; l'ID selezionato appare nell'etichetta |
| **Frecce sinistra/destra** | Sposta l'oggetto selezionato di −/+0,1 lungo X |
| **Frecce su/giù** | Sposta l'oggetto selezionato di −/+0,1 lungo Z |
| **PagSu / PagGiù** | Sposta l'oggetto selezionato di +/−0,1 lungo Y |
| **Ctrl+D** | Duplica l'oggetto selezionato e seleziona il nuovo ID |
| **Canc** | Rimuove l'oggetto selezionato, se tutti gli eventi futuri restano validi |
| **Spazio** | Mette in pausa o riprende la simulazione |
| **F5** | Riavvia l'anteprima dal tick 0 e la mette in movimento |
| **Ctrl+S** | Salva scena iniziale, timeline e controlli delle tre camere |
| **R** | Rilegge il progetto dal disco se non ci sono modifiche da salvare; riparte in pausa al tick 0 |
| **H** | Mostra/nasconde le etichette |
| **C** | Alterna Center e Conservative9 per l'anteprima |
| **Esc** o chiusura della finestra | Chiude solo se non ci sono modifiche da salvare |
| **Maiusc+Esc** | Scarta esplicitamente le modifiche in memoria e chiude |

Una modifica a un oggetto interessa la **scena iniziale**, anche se la timeline
lo ha già spostato o rimosso nell'anteprima. Se accettata, ricrea l'anteprima in
pausa al tick 0 e azzera la history dei glifi. Gli eventi al tick 0 vengono
applicati alla successiva composizione. Le tre camere conservano i controlli
scelti. Un errore mantiene il progetto precedente e viene mostrato nell'etichetta
e nel terminale. Non esiste ancora un comando di annullamento: `save-as` permette
di conservare una copia prima di una serie di modifiche.

L'asterisco nell'etichetta indica modifiche da salvare. La chiusura non salva
automaticamente. Anche la scadenza di `--seconds` lascia aperta una finestra con
modifiche non salvate; dopo averle salvate, chiudere normalmente. Per ricaricare
un progetto modificato in un altro programma, salvare o scartare prima le
modifiche locali. In caso di conflitto sul file, vedere la sezione sul
salvataggio qui sotto.

La selezione riguarda gli oggetti iniziali, non quelli inseriti solo dalla
timeline. Forma, dimensioni, materiale, rotazione, proiezione delle camere ed
eventi si modificano con `author apply` o con i comandi dedicati. I controlli
della finestra non aggiungono collisioni o movimento di un personaggio.

`P` non attiva percorsi automatici nella finestra di un progetto;
`--camera-path coverage` e un `--orbit-speed` non nullo sono respinti in questa
modalità. In questo modo la camera salvata corrisponde ai controlli usati
nell'anteprima. I percorsi espliciti restano disponibili per catture headless.

## Modifiche dal terminale

Chiudere l'anteprima, o salvarla e poi ricaricarla con **R**, quando si modifica
lo stesso progetto dal terminale. Ogni comando seguente valida e salva subito
il candidato completo. Gli ID indicati sono quelli del campione creato da `new`:

```powershell
.\target\release\author.exe translate my-room.json 7 0.2 0 0
.\target\release\author.exe material my-room.json 7 36
.\target\release\author.exe duplicate my-room.json 7
.\target\release\author.exe inspect my-room.json
.\target\release\author.exe save-as my-room.json my-room-backup.json
```

`translate` aggiunge lo spostamento alla posizione iniziale e conserva
l'eventuale `motion`. `material` assegna un materiale già presente.
`duplicate` stampa il nuovo ID; copia anche la posizione, quindi la copia
inizialmente si sovrappone all'originale. Spostarla usando l'ID stampato.
Il nuovo ID evita quelli già usati dalla scena e quelli citati negli eventi
futuri. `save-as` copia il progetto salvato verso un file nuovo: non recupera
modifiche ancora nella memoria di una finestra.

Per rimuovere un oggetto usare, ad esempio, `author.exe remove my-room.json 7`.
La rimozione viene respinta se un evento futuro richiede ancora quell'ID. Se
occorre cambiare insieme oggetto ed eventi, usare un solo `apply`.

Per inserire un oggetto, salvare questo contenuto UTF-8 in `new-object.json`:

```json
{
  "id": 100,
  "name": "Small stone block",
  "shape": "box",
  "material": 32,
  "position": [0.0, 0.4, 1.5],
  "half_extents": [0.4, 0.4, 0.4],
  "rotation_y": 0.2
}
```

```powershell
.\target\release\author.exe insert my-room.json new-object.json
```

L'ID deve essere libero e il materiale presente. `half_extents` contiene le
semidimensioni per una box, i raggi per un'ellissoide. Le due forme supportate
sono `box` ed `ellipsoid`; il JSON completo dell'oggetto segue lo
[schema del client](README.md#dati-comuni-e-compatibilità-v1).

## Una modifica coordinata a materiale, camera ed eventi

Questo esempio parte da un progetto appena creato con `new`. Nel campione
l'oggetto ID 7 si trova all'indice 6 dell'array `scene.objects`. Per altri
progetti controllare prima l'output di `inspect`: **un indice JSON non è un ID**.

Salvare il seguente JSON in `edit-room.json`:

```json
{
  "version": 1,
  "operations": [
    {
      "op": "replace",
      "path": "/scene/objects/6/material",
      "value": 36
    },
    {
      "op": "replace",
      "path": "/scene/cameras/third/fov_y_degrees",
      "value": 60.0
    },
    {
      "op": "replace",
      "path": "/camera_controls/third/orbit",
      "value": 0.2
    },
    {
      "op": "replace",
      "path": "/updates/events",
      "value": [
        {
          "tick": 30,
          "operations": [
            {
              "op": "transform",
              "id": 7,
              "position": [1.6, 0.9, 0.65],
              "half_extents": [0.7, 0.6, 0.55],
              "rotation_y": -0.35
            }
          ]
        }
      ]
    }
  ]
}
```

```powershell
.\target\release\author.exe apply my-room.json edit-room.json
.\target\release\ascii-material-room.exe --project my-room.json --validate
.\target\release\ascii-material-room.exe --project my-room.json --camera third
```

Le operazioni vengono applicate nell'ordine a un candidato privato. Solo al
termine sono validati scena, camere e **intera timeline**; se una parte fallisce,
nessuna modifica del batch viene salvata. L'esempio sostituisce tutta la lista
degli eventi e programma la trasformazione al secondo 1, cioè al tick 30.
Le trasformazioni della timeline sono assolute e rimuovono l'eventuale `motion`
dell'oggetto quando vengono eseguite. Il file del progetto conserva comunque
la scena iniziale e gli eventi, non la scena risultante dopo il playback.

`apply` supporta `add`, `remove` e `replace`. I percorsi devono iniziare con
`/scene`, `/updates` o `/camera_controls`; non aprono file esterni.
`add` su `/scene/objects/-` inserisce in fondo all'array. `replace` e `remove`
richiedono una destinazione esistente. Non sono supportati `move`, `copy` o
`test`. Un batch ammette al massimo 128 operazioni, un percorso fino a 512 byte
e 32 componenti. Gli escape JSON Pointer sono `~0` per `~` e `~1` per `/`.

## Camere e formato del progetto

Il progetto v1 richiede esattamente `version`, `scene`, `updates` e
`camera_controls`. `scene` usa il formato v2; `updates` il formato v1. Ogni
record `first`, `third`, `overhead` di `camera_controls` richiede:

```json
{
  "orbit": 0.0,
  "offset": [0.0, 0.0, 0.0],
  "zoom": 1.0
}
```

`scene.cameras` conserva occhio, target e proiezione di base. I controlli
vengono salvati separatamente, così una rilettura usa la stessa composizione
della camera senza ricavare una nuova base da punti arrotondati. Ogni vista
ha controlli indipendenti. `orbit` è in radianti, fra −1024 e 1024; ciascuna
componente di `offset` è fra −10000 e 10000; `zoom` è fra 0,01 e 100.
Tutti i valori devono essere finiti. Campi aggiuntivi, anche `pitch`, vengono
respinti.

La rotazione mantiene fermo l'occhio in prima persona e il target nelle altre
viste. `zoom` regola la distanza occhio-target e, dall'alto, anche l'estensione
ortografica. Non modifica il campo visivo prospettico; in prima persona non è
uno zoom ottico. Per cambiare l'inquadratura prospettica modificare
`fov_y_degrees` nella camera di base. Per cambiare l'inclinazione modificare
`eye` e `target` della base; non c'è un tasto pitch nella finestra.

Il progetto UTF-8 è limitato a 2 MiB anche dopo la formattazione di salvataggio;
scena, timeline e file patch hanno ciascuno limite di 1 MiB. Restano i limiti
dei formati interni: fino a 128 oggetti e 32 materiali; fino a 256 eventi,
128 operazioni per evento, tick interi crescenti fra 0 e 2.592.000 a 30 Hz.
Campi sconosciuti, chiavi duplicate, ID in conflitto e riferimenti invalidi
sono respinti. I limiti completi di geometria, superfici e camere sono nel
[README](README.md). Una modifica della scena iniziale deve lasciare valida
anche l'ultima operazione futura.

## Salvataggio e conflitti

Il salvataggio acquisisce un file cooperativo `<progetto>.lock`, scrive un
temporaneo nella stessa directory, confronta i byte del progetto con quelli
letti all'apertura o all'ultimo salvataggio e pubblica la sostituzione.
Un errore di validazione o un conflitto rilevato lascia intatto il file
destinazione. In finestra le modifiche non salvate restano in memoria.
Anche un salvataggio senza modifiche controlla eventuali cambiamenti esterni.

I programmi che rispettano lo stesso lock si escludono a vicenda. Un altro
editor che ignora il lock può ancora scrivere fra l'ultimo controllo e la
sostituzione: questa procedura **non è un compare-and-swap del filesystem**
e non promette resistenza a interruzioni di alimentazione. Evitare scritture
contemporanee da altri programmi. Lock esistenti o rimasti dopo un arresto
non vengono sottratti automaticamente; verificarne il proprietario prima
di un intervento manuale.

Se un salvataggio in finestra incontra un file cambiato esternamente, non lo
sovrascrive. Non esiste un merge automatico o un salvataggio forzato: conservare
separatamente la versione esterna e risolvere il conflitto prima di riprovare,
oppure scartare esplicitamente le modifiche locali e riaprire. Il comando
`save-as` copia la versione su disco e non può estrarre la memoria di una
finestra rimasta aperta.

## Catturare le tre viste dello stesso progetto

```powershell
.\target\release\ascii-material-room.exe --project my-room.json --sequence 90 --camera all --profile balanced --workers 8 --output evidence/my-project-three-views
```

Ogni vista riparte dalla stessa scena e timeline, con i propri controlli
salvati. Il manifest conserva il progetto effettivamente caricato. In
headless `--camera-path coverage` o `--orbit-speed 0.025` possono applicare
un percorso aggiuntivo per la cattura: è transitorio, dichiarato nel report
e non viene scritto nel progetto. Novanta tick simulati a 30 Hz non sono una
misura di FPS. Le prove di compatibilità e costo rimangono distinte dalla
qualità artistica e dalla verifica su un secondo computer.
