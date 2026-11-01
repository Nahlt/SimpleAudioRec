# SimpleAudioRec — Specifica dei requisiti v1

| | |
|---|---|
| Versione documento | 0.2 |
| Data | 2026-11-01 |
| Piattaforma v1 | Windows 11 x64 |
| Licenza | GPLv3 |
| Repository | `Nahlt/SimpleAudioRec` |

Priorità usate: **M** = Must (obbligatorio v1), **S** = Should (nice to have v1, si fa se fattibile senza compromettere i Must), **V2** = rimandato.

### Storico versioni

| Versione | Modifiche |
|---|---|
| 0.1 | Prima stesura. |
| 0.2 | Nome dell'app: *SimpleAudioRec* (era *SimpleRec*). Il "predefinito" segue il dispositivo **per comunicazioni** (RF-01/02/03). Il mute agisce su **tutti** i microfoni attivi (RF-20, RF-23, RF-24). Aggiunti il formato mono mixato, l'opzione "genera entrambi i formati" e il suffisso `_stereo`/`_mono` nel nome (RF-44a/b, RF-41). Normalizzazione di **loudness** al posto di quella di picco (RF-45). Aggiunti il salvataggio automatico (RF-48), i **segnalibri** (§3.8), l'istanza singola, la posizione della finestra e i tag ID3. Aggiunte la pipeline di build e la distribuzione (§6.1). |

---

## 1. Scopo

SimpleAudioRec è un'applicazione desktop minimale per registrare l'audio del PC durante meeting online (Microsoft Teams in primis, ma anche Zoom, Meet, Webex, ecc.). Registra contemporaneamente:

- l'audio **in uscita** (ciò che si sente: gli altri partecipanti);
- l'audio **in ingresso** dal microfono (la propria voce).

Il risultato è un file MP3: stereo con i due flussi su canali separati (pensato per la trascrizione) e/o mono mixato (pensato per il riascolto).

## 2. Contesto e vincoli

| ID | Vincolo |
|---|---|
| C-01 | Uso personale, singolo utente. |
| C-02 | Applicazione **portable**: un solo `SimpleAudioRec.exe`, nessun installer, nessun runtime da installare, nessun diritto di amministratore richiesto. |
| C-03 | Priorità assoluta: funzionamento affidabile su Windows 11. Il codice deve essere strutturato per un futuro porting su Linux (V2), senza sacrificare la qualità su Windows. |
| C-04 | Licenza GPLv3. Tutte le dipendenze devono essere compatibili con GPLv3. |
| C-05 | Nessun promemoria di consenso alla registrazione. |
| C-06 | L'API locale di Teams per app di terze parti non è disponibile nell'ambiente dell'utente: l'integrazione con il mute di Teams non è percorribile. Il mute è gestito a livello di sistema (§3.3). |

## 3. Requisiti funzionali

### 3.1 Sorgenti audio

| ID | Pri | Requisito |
|---|---|---|
| RF-01 | M | L'utente sceglie il **dispositivo di input** (microfono) dal menu impostazioni. Opzioni: "Predefinito per comunicazioni", "Predefinito di sistema" + elenco dei dispositivi di acquisizione attivi. **Default: "Predefinito per comunicazioni"**, perché Teams e i tool simili usano di norma quel dispositivo. |
| RF-02 | M | L'utente sceglie il **dispositivo di output** da registrare (in loopback) dal menu impostazioni. Opzioni: "Predefinito per comunicazioni", "Predefinito di sistema" + elenco dei dispositivi di riproduzione attivi. **Default: "Predefinito per comunicazioni"**. |
| RF-03 | M | Con una delle opzioni "Predefinito…", se durante la registrazione Windows cambia il dispositivo predefinito del ruolo corrispondente (es. collegamento di cuffie Bluetooth), SimpleAudioRec passa automaticamente al nuovo dispositivo senza interrompere la registrazione. Eventuali secondi di transizione sono riempiti con silenzio, mantenendo l'allineamento temporale. |
| RF-04 | M | Se un dispositivo scelto esplicitamente viene scollegato durante la registrazione, il relativo canale prosegue in silenzio, la UI mostra un avviso e la cattura riprende automaticamente se il dispositivo torna disponibile. |
| RF-05 | S | Per l'output, in alternativa al dispositivo, l'utente può scegliere **"Solo un'applicazione"** e selezionarla da un elenco dei processi con audio attivo (es. Teams). Viene catturato l'audio dell'intero albero di processi dell'applicazione (necessario per Teams, che riproduce l'audio da processi figli). |
| RF-06 | M | Il cambio delle sorgenti dal menu impostazioni è disabilitato durante una registrazione (resta attivo solo l'inseguimento automatico di RF-03). |

### 3.2 Registrazione

| ID | Pri | Requisito |
|---|---|---|
| RF-10 | M | **Avvia**: inizia la registrazione da pulsante o hotkey. |
| RF-11 | M | **Pausa / Riprendi**: in pausa l'audio non viene registrato; alla ripresa il file prosegue senza buchi (il tratto in pausa semplicemente non esiste nel file). |
| RF-12 | M | **Stop**: termina la registrazione e avvia il salvataggio (§3.5). |
| RF-13 | M | Nessun limite di durata della registrazione. |
| RF-14 | M | Un timer mostra la durata registrata (pause escluse) nel formato `hh:mm:ss`. |
| RF-15 | M | Nella registrazione grezza il canale **sinistro = microfono** e il canale **destro = audio di output**. Le sorgenti stereo vengono convertite in mono prima del posizionamento sul canale. |
| RF-16 | M | I due canali restano allineati nel tempo per tutta la registrazione, anche con orologi diversi dei due dispositivi (vedi RNF-04). Quando non c'è audio in riproduzione il canale destro contiene silenzio (Windows in loopback non invia dati in assenza di audio: va compensato). |
| RF-17 | M | Se si chiude la finestra durante una registrazione viene chiesto: "Registrazione in corso. Fermare e salvare?" → *Ferma e salva* / *Annulla*. |
| RF-18 | M | Un hotkey non applicabile allo stato corrente viene ignorato senza messaggi (es. *Pausa* da fermo, *Avvia* durante una registrazione). |

### 3.3 Mute del microfono

| ID | Pri | Requisito |
|---|---|---|
| RF-20 | M | Un pulsante e un hotkey globale attivano/disattivano il **mute del microfono a livello di sistema** su **tutti i dispositivi di acquisizione attivi**, non solo su quello registrato. Così il mute è efficace qualunque microfono usi Teams (o un altro tool), anche se diverso da quello registrato da SimpleAudioRec. Effetto: il microfono è silenziato per tutte le applicazioni e per la registrazione stessa. Un microfono collegato mentre il mute di SimpleAudioRec è attivo viene silenziato anch'esso. |
| RF-21 | M | Il mute funziona sempre quando SimpleAudioRec è aperto, anche fuori da una registrazione. |
| RF-22 | M | Durante il mute il canale del microfono contiene silenzio (allineamento temporale preservato). |
| RF-23 | M | La UI riflette lo stato di mute del **microfono registrato**, inclusi i cambi fatti dall'esterno (tasto mute hardware, impostazioni rapide di Windows). Alla successiva pressione di Mute tutti i microfoni vengono allineati allo stesso stato. |
| RF-24 | M | SimpleAudioRec memorizza su disco quali dispositivi ha silenziato lui ("possesso" del mute). Alla chiusura rimuove il mute solo da quei dispositivi; un mute applicato dall'esterno prima non viene toccato. Se l'app si è chiusa in modo anomalo lasciando dei microfoni in mute, al successivo avvio il mute viene rimosso da quei dispositivi e l'utente viene avvisato. |
| RF-25 | — | Nota (limite accettato): il mute applicato dal pulsante interno di Teams (o di altri tool) non è rilevabile; in quel caso la propria voce viene comunque registrata. Per escluderla dalla registrazione va usato il mute di SimpleAudioRec. Non è richiesto che Teams mostri lo stato di mute. |

### 3.4 Indicatori di stato

| ID | Pri | Requisito |
|---|---|---|
| RF-30 | M | **Pallino rosso**: registrazione in corso. In pausa il pallino è spento/grigio con la scritta "In pausa". |
| RF-31 | M | **Pallino verde**: si illumina quando viene rilevato audio dall'output (livello sopra soglia, es. −50 dBFS, con breve tenuta ~150 ms per evitare sfarfallio). |
| RF-32 | M | **Pallino arancione**: si illumina quando viene rilevato audio dal microfono (stessa logica). Durante il mute resta spento e accanto compare un'icona di microfono barrato. |
| RF-33 | M | Gli indicatori verde e arancione sono attivi durante la registrazione e durante la pausa (per verificare l'audio prima di riprendere). Nessun VU meter, nessuna regolazione del volume. |
| RF-34 | S | Con la finestra ridotta a icona, lo stato è visibile dalla barra delle applicazioni: overlay con pallino rosso sull'icona e timer nel titolo della finestra. |

### 3.5 Salvataggio

| ID | Pri | Requisito |
|---|---|---|
| RF-40 | M | Allo Stop compare il **pannello di salvataggio** nella finestra principale, con: casella "Genera entrambi i formati (stereo e mono)" precompilata dalle impostazioni; elenco dei segnalibri con descrizione modificabile (§3.8); pulsanti *Salva…* e *Scarta*. *Salva…* apre la finestra standard di Windows "Salva con nome". |
| RF-41 | M | Nome proposto: data e ora di inizio registrazione + formato, `AAAA-MM-GG_hh-mm_stereo.mp3` oppure `AAAA-MM-GG_hh-mm_mono.mp3` (es. `2026-11-01_14-30_stereo.mp3`). Con "entrambi i formati" l'utente sceglie un solo nome nel dialogo; il secondo file viene creato accanto, con lo stesso nome e il suffisso dell'altro formato. |
| RF-42 | M | Cartella proposta: l'ultima usata (alla prima esecuzione: Documenti). |
| RF-43 | M | Se l'utente annulla il salvataggio o preme *Scarta*: conferma "Scartare la registrazione?" → *Sì* (eliminata) / *No* (torna al pannello di salvataggio). |
| RF-44 | M | Formato MP3, CBR, 48 kHz. Bitrate configurabile: 64 / 96 / 128 / 160 / 192 kbps. **Default 96 kbps** (~43 MB/ora in stereo). |
| RF-44a | M | **Formato predefinito** scelto nelle impostazioni: **"Stereo separato"** (default; canale sinistro = microfono, destro = output, **stereo puro, non joint stereo**, per evitare contaminazione tra i canali; adatto alla trascrizione) oppure **"Mono mixato"** (somma dei due canali; adatto al riascolto in cuffia). Il mono usa metà del bitrate impostato, con un minimo di 48 kbps (es. 96 → 48 kbps). |
| RF-44b | M | Impostazione **"Genera entrambi i formati"** (default: disattivata), che precompila l'omonima casella del pannello di salvataggio (RF-40). |
| RF-45 | M | **Normalizzazione di loudness** opzionale, attivabile dal menu impostazioni (default: attiva). Applicata **separatamente su ogni canale**: loudness integrata (ITU-R BS.1770 / EBU R128) portata a **−16 LUFS**, guadagno limitato tra −10 e +15 dB, limiter di picco a −1 dBFS che interviene solo sui picchi oltre soglia (nessuna compressione dinamica). Un canale silenzioso (sotto la soglia di gating) non viene amplificato. Nel mono la somma dei canali normalizzati passa di nuovo dal limiter. |
| RF-46 | M | La codifica avviene alla fine; per registrazioni lunghe viene mostrata una barra di avanzamento. |
| RF-47 | M | Avviso quando lo spazio libero sul disco dei file temporanei scende sotto la soglia necessaria (< 1 GB o < 30 minuti di registrazione stimata). La registrazione non viene interrotta. |
| RF-48 | M | Impostazione **"Salva automaticamente nell'ultima cartella"** (default: disattivata). Se attiva, allo Stop non compaiono né il pannello né il dialogo: i file vengono salvati con il nome proposto, nei formati previsti dalle impostazioni, nell'ultima cartella usata. Se un file con lo stesso nome esiste già, si aggiunge un suffisso ` (2)`, ` (3)`… Al termine un messaggio nella finestra indica dove è stato salvato il file. |
| RF-49 | M | Ogni MP3 contiene **tag ID3v2**: titolo `Registrazione AAAA-MM-GG hh:mm`, data, durata, software `SimpleAudioRec`, segnalibri come capitoli (§3.8). |

### 3.6 Robustezza e recupero

| ID | Pri | Requisito |
|---|---|---|
| RF-50 | M | Durante la registrazione l'audio è scritto progressivamente su un file temporaneo non compresso (PCM 16 bit stereo 48 kHz, ~690 MB/ora), aggiornato in modo che resti leggibile anche dopo un crash. I segnalibri sono salvati progressivamente accanto al file temporaneo. |
| RF-51 | M | All'avvio, se viene trovata una registrazione non salvata: "Trovata una registrazione non salvata del <data> (<durata>). Recuperarla?" → *Recupera* (porta al pannello di salvataggio §3.5, segnalibri inclusi) / *Scarta* (con conferma). |
| RF-52 | M | Dopo un salvataggio riuscito o uno scarto confermato, i file temporanei vengono eliminati. |
| RF-53 | M | **Istanza singola**: se si avvia una seconda volta l'exe mentre SimpleAudioRec è già aperto, si porta in primo piano la finestra esistente e la seconda istanza termina (evita conflitti su hotkey e mute). |

### 3.7 Hotkey globali

Combinazioni attive anche quando SimpleAudioRec non è in primo piano (es. mentre si usa Teams).

| ID | Pri | Azione | Default |
|---|---|---|---|
| RF-60 | M | Avvia registrazione | `Ctrl+Alt+R` |
| RF-61 | M | Pausa / Riprendi | `Ctrl+Alt+P` |
| RF-62 | M | Stop | `Ctrl+Alt+S` |
| RF-63 | M | Mute / Unmute microfono | `Ctrl+Alt+M` |
| RF-65 | M | Aggiungi segnalibro | `Ctrl+Alt+B` |
| RF-64 | M | Gli hotkey sono modificabili dal menu impostazioni. Se una combinazione è già occupata da un'altra applicazione, l'utente viene avvisato. |

### 3.8 Segnalibri

| ID | Pri | Requisito |
|---|---|---|
| RF-80 | M | Durante una registrazione (anche in pausa) l'utente aggiunge un **segnalibro** con il pulsante *Segna* o con l'hotkey `Ctrl+Alt+B`. Il segnalibro registra l'istante corrente della registrazione (tempo registrato, pause escluse). |
| RF-81 | M | Ogni segnalibro riceve una descrizione automatica ("Segnalibro 1", "Segnalibro 2"…), modificabile nel pannello di salvataggio. Alla pressione la UI dà un breve riscontro visivo (es. "🔖 3" accanto al timer). |
| RF-82 | M | Al salvataggio, se c'è almeno un segnalibro: (a) i segnalibri vengono scritti come **capitoli ID3** (frame `CHAP`/`CTOC`) in ogni MP3 generato; (b) viene creato un file di testo accanto all'MP3, con lo stesso nome base e il suffisso `_segnalibri.txt` (es. `2026-11-01_14-30_segnalibri.txt`), una riga per segnalibro nel formato `hh:mm:ss  descrizione`. |

### 3.9 Impostazioni

| ID | Pri | Requisito |
|---|---|---|
| RF-70 | M | Menu impostazioni aperto da un'icona a ingranaggio nella finestra principale. Contiene: dispositivo di input, sorgente di output (RF-02/RF-05), formato predefinito (RF-44a), genera entrambi i formati (RF-44b), bitrate, normalizzazione, salvataggio automatico (RF-48), hotkey, lingua. |
| RF-71 | M | Le impostazioni (inclusa l'ultima cartella di salvataggio e la posizione della finestra) sono in un file `SimpleAudioRec.toml` accanto all'exe; se la cartella non è scrivibile, in `%APPDATA%\SimpleAudioRec\`. I file temporanei di RF-50 e lo stato di possesso del mute (RF-24) seguono la stessa regola (sottocartella `recovery`). |
| RF-72 | M | Lingua dell'interfaccia: italiano e inglese. Default: lingua di sistema (fallback inglese), modificabile. |

## 4. Interfaccia utente

- Finestra normale, compatta, dimensione fissa; **non** sempre in primo piano; riducibile nella barra delle applicazioni; **nessuna** icona nella system tray; nessun avvio automatico con Windows.
- La posizione della finestra viene ricordata tra un avvio e l'altro (se lo schermo non è più disponibile, la finestra viene centrata).
- Icona dell'app originale, pulita, in stile Windows 11 (Fluent).
- Contenuto della finestra principale:

```
┌──────────────────────────────────────┐
│  ● REC   00:42:17   🔖 3          ⚙  │   ● rosso = in registrazione
│                                      │
│  ● Output        ● Microfono   🎙/🔇  │   verde / arancione + stato mute
│                                      │
│   [ ⏺ Rec ]  [ ⏸ Pausa ]  [ ⏹ Stop ]  │
│        [ 🔇 Mute ]   [ 🔖 Segna ]     │
└──────────────────────────────────────┘
```

- Stato dei pulsanti: *Rec* attivo solo da fermo; *Pausa* (che diventa *Riprendi*), *Stop* e *Segna* attivi solo durante la registrazione; *Mute* sempre attivo.
- Pannello di salvataggio (dopo lo Stop, RF-40):

```
┌──────────────────────────────────────┐
│  Registrazione terminata — 00:42:17  │
│  [x] Genera entrambi i formati       │
│  Segnalibri:                         │
│   00:05:12  [Segnalibro 1         ]  │
│   00:31:40  [Decisione budget     ]  │
│                                      │
│        [ Salva… ]   [ Scarta ]       │
└──────────────────────────────────────┘
```

## 5. Requisiti non funzionali

| ID | Requisito |
|---|---|
| RNF-01 | Un solo `.exe` per Windows x64, senza dipendenze esterne (librerie e runtime C collegati staticamente), dimensione indicativa ≤ 15 MB. |
| RNF-02 | Avvio in meno di 1 s. |
| RNF-03 | Consumo durante la registrazione: CPU media < 3% su un PC moderno, RAM < 100 MB. |
| RNF-04 | Desincronizzazione tra i canali < 100 ms dopo 3 ore di registrazione (compensazione della deriva tra gli orologi dei dispositivi tramite ricampionamento adattivo). |
| RNF-05 | Nessuna perdita di audio udibile (drop-out) in condizioni normali di carico. |
| RNF-06 | Nessuna telemetria e nessuna connessione di rete. |
| RNF-07 | L'exe non è firmato digitalmente: l'avviso di SmartScreen alla prima esecuzione è accettato ("Ulteriori informazioni → Esegui comunque"). Su PC aziendali le policy (AppLocker, WDAC) potrebbero richiedere l'autorizzazione dell'IT. |

## 6. Architettura tecnica

**Stack**: Rust, interfaccia con `egui`/`eframe`.

Struttura a moduli, con l'audio dietro un'astrazione per il futuro porting su Linux:

| Modulo | Responsabilità |
|---|---|
| `core` | Macchina a stati della registrazione, mixing su due canali, sincronizzazione/ricampionamento, file temporaneo e recupero, segnalibri, normalizzazione di loudness, mix mono, codifica MP3, tag ID3, impostazioni, traduzioni. Indipendente dalla piattaforma. |
| `audio` (trait `AudioBackend`) | Elenco dispositivi/applicazioni, cattura microfono, cattura loopback, cattura per processo, mute di sistema, notifiche di cambio dispositivo/mute. |
| `audio::wasapi` | Implementazione Windows: WASAPI (loopback, process loopback), `IAudioEndpointVolume` per il mute, `IMMNotificationClient` per i cambi di dispositivo. |
| `audio::pipewire` | V2 — implementazione Linux. |
| `app` | UI egui, hotkey globali, dialoghi, istanza singola, overlay nella barra delle applicazioni. |

Dipendenze previste (tutte compatibili GPLv3): `eframe`/`egui` (MIT/Apache-2.0), `windows` (MIT/Apache-2.0), LAME tramite binding Rust, collegata staticamente (LGPL), `global-hotkey` (MIT/Apache-2.0), `rfd` per i dialoghi (MIT), `serde` + `toml` (MIT/Apache-2.0), `ebur128` per la misura di loudness (MIT), `id3` per i tag (MIT). Il ricampionamento adattivo (conversione a 48 kHz e compensazione della deriva) è implementato nel modulo `core` (sinc polifase con finestra di Kaiser) invece di usare `rubato`, per avere un componente piccolo e testato sul caso d'uso. L'interfaccia usa il backend OpenGL di eframe (`glow`), più leggero e rapido all'avvio. Le licenze di terze parti sono raccolte in `THIRD_PARTY_NOTICES.txt`, distribuito con l'exe.

### 6.1 Build e distribuzione

- Sviluppo con commit diretti sul ramo `main`.
- Compilazione locale di verifica (anche cross-compilata da Linux) e test unitari del modulo `core`.
- **GitHub Actions** su runner `windows-latest` (target `x86_64-pc-windows-msvc`, CRT statico):
  - a ogni push su `main` e manualmente: test + build, exe scaricabile dagli *Artifacts* dell'esecuzione;
  - a ogni tag `v*` (es. `v0.1.0`): anche una **Release** con lo zip (`SimpleAudioRec.exe`, `LEGGIMI.txt`, `LICENSE`, `THIRD_PARTY_NOTICES.txt`).
- I test funzionali con l'audio reale si fanno su un PC Windows 11 (i runner non hanno dispositivi audio).

## 7. Fuori ambito v1 / roadmap

| Pri | Funzione |
|---|---|
| V2 | Avvio automatico della registrazione quando inizia un meeting. |
| V2 | Versione Linux (PipeWire/PulseAudio). |
| V2 | Integrazione con l'API di Teams (stato del mute reale) se diventa disponibile. |
| V2 | Cancellazione dell'eco (uso con altoparlanti anziché cuffie). |
| — | Trascrizione: fuori ambito; il formato stereo separato è pensato per strumenti esterni. |

## 8. Rischi e note

| Rischio | Mitigazione |
|---|---|
| Uso con altoparlanti: il microfono riprende anche le voci remote (eco nel canale sinistro). | Consigliate le cuffie; cancellazione dell'eco in V2. |
| Cuffie Bluetooth: all'apertura del microfono Windows può passare al profilo "vivavoce" (qualità ridotta) e cambiare dispositivo. | Gestito da RF-03; limite noto dell'hardware/driver. |
| Teams configurato su un dispositivo specifico diverso dal predefinito per comunicazioni: SimpleAudioRec registrerebbe il dispositivo sbagliato. | Default "Predefinito per comunicazioni" (RF-01/02); scelta esplicita del dispositivo nelle impostazioni; il mute agisce comunque su tutti i microfoni (RF-20). |
| Mute dal pulsante interno di Teams non rilevabile. | Uso del mute di SimpleAudioRec (RF-25). |
| Microfoni lasciati in mute di sistema dopo un crash. | RF-24 (possesso del mute salvato su disco). |
| Exe non firmato bloccato su PC aziendali gestiti. | RNF-07; eventuale firma valutabile in futuro. |
| Cattura per applicazione (RF-05) su processi con audio in processi figli. | Cattura dell'intero albero di processi; requisito S, verificato in fase di sviluppo. |

## 9. Criteri di accettazione

1. Avviando una registrazione durante un meeting Teams di prova, il file `_stereo.mp3` salvato contiene la propria voce **solo** sul canale sinistro e quella dei partecipanti **solo** sul destro; il file `_mono.mp3` (se generato) contiene entrambe.
2. Gli indicatori verde e arancione si accendono parlando o riproducendo audio, e restano spenti nel silenzio.
3. `Ctrl+Alt+M` con Teams in primo piano: i partecipanti non sentono più l'utente e il canale sinistro è in silenzio da quel momento; premendolo di nuovo tutto si ripristina. Funziona anche se Teams usa un microfono diverso da quello registrato.
4. Una pausa di 1 minuto in una registrazione di 5 minuti produce un file di 4 minuti, senza buchi.
5. Scollegando/collegando le cuffie durante la registrazione, questa prosegue e segue il nuovo dispositivo predefinito.
6. Terminando forzatamente il processo durante una registrazione, al riavvio viene proposto il recupero e il file recuperato contiene l'audio (e i segnalibri) fino al momento del crash; i microfoni non restano in mute.
7. In una registrazione di 2 ore i canali restano sincronizzati (scarto < 100 ms, verificato con un segnale di riferimento all'inizio e alla fine).
8. L'exe funziona copiato in una cartella qualsiasi (anche una chiavetta USB) su un Windows 11 pulito, senza installazioni aggiuntive.
9. Tre segnalibri aggiunti durante una registrazione compaiono nel file `_segnalibri.txt` con i tempi corretti (±1 s) e come capitoli in un player che li supporta.
10. Con "Salva automaticamente" attivo, lo Stop salva i file senza dialoghi nell'ultima cartella, senza sovrascrivere file esistenti.
11. Avviando l'exe una seconda volta, compare la finestra già aperta e non si crea una seconda istanza.
