# Leitfaden: Evidenzbasierte Lernpakete (SCORM) gestalten

Entwurf · Oct 24, 2026 · @Dennis Busch

## Zweck und Kurzfassung

Gute Lernpakete entstehen nicht durch mehr Technik, sondern durch wenige gut belegte Gestaltungsprinzipien, die konsequent umgesetzt werden. Dieser Leitfaden bündelt diese Prinzipien für das Projekt PING und für künftige Pakete in anderen Fächern. Er richtet sich ausdrücklich auch an Kolleginnen und Kollegen, die bisher weder Software entwickelt noch KI über einfache Chatbots hinaus genutzt haben.

Der Leitfaden hat drei Teile: **A** fasst die Forschungslage zusammen, **B** wählt daraus die für uns relevanten Prinzipien aus und wägt sie gegen den Aufwand ab, **C** ist der eigentliche Leitfaden mit Prozess, Paketstruktur, Checklisten und dem Vorschlag für ein Skill-Set.

**Die zehn Kernsätze**

1. **Ganze Aufgaben als Rückgrat:** Jedes Paket baut auf einer authentischen beruflichen bzw. lebensweltlichen Handlungssituation auf, nicht auf einer Stoffsammlung.
2. **Vom Einfachen zum Komplexen, Hilfe wird schrittweise zurückgenommen:** zeigen, begleitet üben, selbstständig können.
3. **Erst vormachen, dann nachmachen:** Novizen brauchen Lösungsbeispiele; wer schon kann, darf sie überspringen.
4. **Entdecken ja, aber geführt:** Freies Ausprobieren ohne Struktur schadet dem Lernen messbar.
5. **Abrufen statt Wiederlesen:** Häufige, unbewertete Kurz-Checks gehören in jedes Kapitel; am Ende ergänzen Lernende ein Merkblatt aus dem Gedächtnis.
6. **Feedback erklärt, statt nur zu bewerten:** „Richtig/falsch“ plus Begründung und nächster Schritt; Lob allein bringt wenig.
7. **Bildschirme entrümpeln (Mayer):** nur, was dem Lernziel dient; Text und Bild zusammen, Inhalte in kleine Schritte geteilt; Wichtiges und der nächste Schritt sind auf einen Blick erkennbar.
8. **Selbststeuerung muss unterstützt werden:** Wahlfreiheit allein verbessert das Lernen nicht; Ziele, Fortschritt und Reflexion schon.
9. **Einfache Verzweigung statt „intelligenter“ Lernpfade:** Gestufte Tipps und Wiederholungsschleifen leisten fast alles, was wir brauchen.
10. **Iterativ bauen und mit Schülern erproben:** früh einen Prototyp testen, nach dem Einsatz die Daten aus der Lernplattform auswerten.

Wer wenig Zeit hat, liest diese zehn Sätze, Teil B (Prinzipienübersicht) und Teil C6 (Checkliste).

## Teil A1: Was die Forschung sagt und wie verlässlich sie ist

Für die Gestaltung digitaler Lernangebote gibt es eine Handvoll gut untersuchter Ansätze. Sie beantworten unterschiedliche Fragen und ergänzen sich; am besten belegt sind die Regeln zur Bildschirmgestaltung (Mayer) und der Aufbau eines Kurses über ganze, realistische Aufgaben (4C/ID).

**Wie man die Zahlen in diesem Leitfaden liest.** Viele Studien geben an, wie viel besser eine Gruppe mit einer bestimmten Methode abschneidet als eine Vergleichsgruppe. Diese sogenannte Effektstärke (abgekürzt *d* oder *g*) lässt sich für den Schulbereich grob so lesen: Unter 0,2 bringt eine Maßnahme praktisch nichts, 0,2 bis 0,4 ist ein kleiner, 0,4 bis 0,6 ein mittlerer und ab 0,6 ein großer Effekt (Kerres 2024, angelehnt an Hattie). Auch ein kleiner Effekt lohnt sich, wenn die Maßnahme wenig Aufwand macht. Drei Vorsichtsregeln:

- **Der Vergleich entscheidet.** Gegenüber herkömmlichem Unterricht sieht fast jedes digitale Angebot gut aus, gegenüber einem anderen guten Lernprogramm schrumpft der Vorteil. Beispiel: „Intelligente“ Lernsysteme erreichen 0,66 gegenüber Klassenunterricht, aber nur 0,28 gegenüber einfachen interaktiven Lernprogrammen (Kulik & Fletcher 2016, zit. n. [Kerres et al.](https://learninglab.uni-due.de/sites/default/files/KI-Bildung-Sammelband-V1.4_1.pdf)).
- **Große Zahlen aus kleinen Studien sind verdächtig.** Studien mit eindrucksvollen Ergebnissen werden eher veröffentlicht, und viele Zusammenfassungen korrigieren das nicht ([Ropovik et al. 2021](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC8174709/)).
- **Der Reiz des Neuen verfliegt.** Wirkungen neuer Medien werden mit der Zeit kleiner ([Cromley et al.](https://par.nsf.gov/search/author:%22Cromley,%20Jennifer%22)).

**Was digitales Lernen insgesamt bringt.** Michael Kerres fasst in seinem Lehrbuch *Mediendidaktik* (2024, Kap. 4) Jahrzehnte an Vergleichsstudien nüchtern zusammen: Digitales Lernen ist herkömmlichem Unterricht nicht unterlegen und im Mittel leicht überlegen. Ersetzt der Computer den Unterricht, liegt der Effekt bei *d* = 0,31, ergänzt er ihn, bei 0,42 (Tamim et al. 2011). Für Lernpakete sind vier Befunde besonders wichtig:

- Die Kombination mit Präsenzunterricht wirkt besser als reines Online-Lernen.
- Wer allein am Computer lernt, braucht im Schnitt deutlich weniger Zeit (rund ein Viertel bis ein Drittel), bricht aber auch häufiger ab.
- Der Erfolg hängt davon ab, ob Lernende ihr eigenes Lernen überwachen und steuern können.
- Der Reiz des Neuen verfliegt schnell. Wirkt ein Angebot vor allem unterhaltsam, strengen sich Lernende sogar weniger an.

Kerres' Fazit: Nicht das Digitale macht den Erfolg aus, sondern die didaktische Gestaltung.

**Hattie richtig einordnen.** John Hatties *Visible Learning* („Lernen sichtbar machen“) fasst inzwischen über 2.100 Forschungsübersichten zusammen und ordnet Einflussfaktoren in eine Rangliste ([Boulhrir 2025](https://jle.hse.ru/article/download/27821/23245)). Diese Rangliste ist umstritten, weil sehr unterschiedliche Studien zu einer Zahl verrechnet werden ([Johnson & Janzen 2023](https://ices.library.ubc.ca/index.php/criticaled/article/view/186745/185678)). Ein Beispiel: Für Feedback nannte Hattie früher Werte um 0,7; eine genauere Nachrechnung seines eigenen Teams kommt auf 0,48, mit großen Unterschieden je nach Art des Feedbacks ([Wisniewski, Zierer & Hattie 2020](https://opus.bibliothek.uni-augsburg.de/opus4/files/71009/fpsyg-10-03087.pdf)). Hattie taugt deshalb als Landkarte, nicht als Rangliste.

**Die wichtigsten Ansätze im Überblick**

| Ansatz | Worum es geht | Wie gut belegt | Was das für unsere Pakete heißt |
| --- | --- | --- | --- |
| Planungsablauf ADDIE | Ein Lernangebot entsteht in fünf Schritten: Analyse, Entwurf (Design), Entwicklung, Einsatz (Implementierung), Auswertung (Evaluation). | Bewährter Projektablauf, aber keine Lerntheorie; Schritt für Schritt abgearbeitet zu schwerfällig. | Die fünf Schritte nutzen, aber in kurzen Runden mit frühem Probelauf (Teil C1). |
| Lernen an ganzen Aufgaben (Merrill; 4C/ID nach van Merriënboer) | Gelernt wird an vollständigen, realistischen Aufgaben statt an zerstückeltem Stoff. Die Aufgaben werden schwieriger, die Hilfe wird weniger. Hintergrundwissen und Bedienhinweise werden getrennt angeboten. | 4C/ID: große Wirkung (*d* = 0,79), vor allem in Hochschule und Berufsbildung ([Costa, Miranda & Melo 2022](https://repositorio.iscte-iul.pt/bitstream/10071/23015/1/article_81383.pdf)); in einer Studie an einer technischen Schule bessere Ergebnisse im Unterricht wie am Computer (Sarfo & Elen 2007, nach Kerres 2024); Merrill bisher vor allem über Befragungen gestützt ([Frick et al. 2010](https://bio.tedfrick.me/etrd/ETRD2010ImprovingCourseEvaluations.pdf)). | Jedes Paket hat eine durchgehende Handlungssituation; vereinfachtes 4C/ID als Planungsraster. |
| Meisterlehre fürs Denken (Cognitive Apprenticeship) | Wie bei einer Ausbilderin: vormachen und dabei laut denken, begleiten, Hilfe schrittweise zurücknehmen, Lernende erklären und reflektieren lassen. | Viele positive Erfahrungsberichte, wenige strenge Vergleichsstudien; Reflexion und Erklärenlassen werden in der Praxis oft vergessen ([Shah & Soosai Raj 2024](https://sigcse2024.sigcse.org/details/sigcse-ts-2024-Papers-1/144/A-Review-of-Cognitive-Apprenticeship-Methods-in-Computing-Education-Research)). | Ausbilderfigur in der Story, gestufte Tipps, Reflexionsfragen. |
| Bildschirmgestaltung nach Mayer (Multimedia-Lernen) | Wir verarbeiten Bilder und Worte über zwei Kanäle mit begrenzter Kapazität. Gute Gestaltung lässt Unnötiges weg, macht Schwieriges handhabbar und regt zum Mitdenken an. | Über 200 Experimente, 15 Gestaltungsregeln ([Mayer](https://edtechbooks.org/docx/10349)); in unabhängiger Nachrechnung im Mittel *g* = 0,37. | Verbindliche Gestaltungsregeln (Teil C3). |
| Gestaltungsorientierte Mediendidaktik (Kerres) | Am Anfang steht ein Lernproblem, nicht die Technik. Die Methode muss zu Zielgruppe, Lernzielen und Rahmen passen. Jedes Lernangebot braucht Orientierung, Lernmaterial und Lernaufgaben; Tests sind optional. Programme, die sich automatisch anpassen („adaptive Systeme“), werden nüchtern bewertet. | Lehrbuch, das die Forschung bündelt (Kerres 2024). Adaptive Systeme: kein belastbarer Vorteil gegenüber gut gemachten interaktiven Programmen, aber hoher Aufwand ([Kerres et al.](https://learninglab.uni-due.de/sites/default/files/KI-Bildung-Sammelband-V1.4_1.pdf)). | Jedes Paket beginnt mit dem Lernproblem (Teil C1); einfache Verzweigungen (Tipps, Wiederholungen) statt KI-Lernpfaden. |
| Lernen sichtbar machen (Hattie) | Klare Ziele, erkennbare Erfolgskriterien, Rückmeldung; die Lehrkraft prüft die Wirkung des eigenen Unterrichts. | Riesige Datenbasis, Methode umstritten (siehe oben). | Lernziele sichtbar machen; Ergebnisse der Pakete auswerten. |

**Zu Kerres, genauer.** Das Lehrbuch *Mediendidaktik* von Michael Kerres (2024) ist das deutschsprachige Standardwerk zum Thema und Grundlage vieler Punkte in diesem Leitfaden. Vier Gedanken sind für uns zentral:

- **Erst das Lernproblem, dann das Medium.** „Bildungsangebote können nur Bildungsprobleme lösen.“ Wer ein Paket plant, klärt zuerst, was die Schüler bisher nicht können und warum. Ein Paket, das vor allem „digital“ sein soll, löst kein Problem.
- **Drei Bausteine sind Pflicht.** Ein Lernangebot braucht Orientierung (worum es geht, wie lange es dauert, was man danach kann), Lernmaterial und Lernaufgaben, die das Lernen tatsächlich auslösen. Austausch mit anderen und Tests sind je nach Ziel sinnvoll, aber kein Muss (sogenanntes 3-2-1-Modell). Wichtig: Lernaufgaben sind keine Testaufgaben, und das sollen die Lernenden auch erkennen.
- **Steuerung bewusst entscheiden.** Ob ein Paket einen festen Weg vorgibt oder freies Erkunden erlaubt, hängt vom Stoff und von den Lernenden ab (Kriterien in Teil C4). Eine harte Sperre („erst weiter, wenn alles richtig ist“) erleben Lernende schnell als Gängelung; besser ist ein Vorschlag, was als Nächstes sinnvoll ist.
- **Automatische Anpassung lohnt sich selten.** Damit sich ein Programm sinnvoll an den Lernstand anpasst, braucht es ein geprüftes Modell, wie Kompetenz wächst, laufende Tests und mehrere Varianten jedes Inhalts. Das ist teuer, und ständig eingestreute Testfragen stören beim Lernen. In einer Vergleichsstudie lernten Schüler mit der anpassenden Variante nicht mehr, waren aber weniger motiviert (Iterbeke et al. 2021). Einfache Anpassungen sind dagegen sinnvoll: Wer wenig Vorwissen hat, bekommt mehr Beispiele; wer es schon kann, darf überspringen.

**Zu Mayer, genauer.** Die Gestaltungsregeln helfen vor allem Lernenden mit wenig Vorwissen. Wer sich schon auskennt, profitiert weniger, manche Hilfen stören sogar (Kalyuga 2007). Das Diagramm zeigt die Wirkung der einzelnen Regeln in Mayers eigenen Studien. Diese Werte stammen überwiegend aus kurzen Laborversuchen und sind deshalb eher als Obergrenze zu lesen.

&#91;embedded content: Mayer (2017), Journal of Computer Assisted Learning; Re-Analyse: Cromley et al. · 12 Prinzipien\]

Für die Praxis heißt das: Die Reihenfolge zeigt, wo sich Aufwand am ehesten lohnt (Text und Bild zusammenbringen, Überflüssiges streichen). Im echten Unterricht sind aber eher kleine bis mittlere Wirkungen realistisch.

## Teil A2: Weitere Befunde, die sich direkt umsetzen lassen

Neben den großen Ansätzen gibt es eine Reihe gut untersuchter Einzelbefunde aus der Lernforschung. Für Lernpakete sind sie besonders nützlich, weil sie sich direkt in Aufgaben und Abläufe übersetzen lassen. Die Tabelle nennt jeweils, womit verglichen wurde, denn davon hängt die Zahl ab.

| Befund | Verglichen mit | Wirkung | Quelle | Was das für Pakete heißt |
| --- | --- | --- | --- | --- |
| Sich selbst abfragen (Übungstests, „Abrufen“) | Nochmal lesen | mittel bis groß, im Labor wie in der Klasse; Multiple Choice wirkt nicht schlechter als offene Fragen | [Adesope et al. 2017](https://education.wsu.edu/documents/2018/01/rethinking-use-tests.pdf/) | Kurz-Checks in jedem Kapitel, ohne Note |
| Wiederholung über mehrere Tage verteilt | Alles auf einmal | *g* = 0,74 | Latimier et al. 2021 (zit. n. [Übersicht](https://studycardsai.com/blog/spaced-repetition-statistics)) | Fragen aus früheren Kapiteln wieder aufgreifen |
| Aufgabentypen gemischt üben | Erst alle Aufgaben einer Sorte, dann die nächste | *g* = 0,42; besonders bei leicht verwechselbaren Aufgaben | [Brunmair & Richter 2019](https://www.x-mol.com/paper/1213055298318831616) | Übungen und Abschlusstest mischen |
| Musterlösungen zuerst | Gleich selbst lösen | *g* = 0,48 (Mathematik) | [Barbieri et al. 2023](https://scholarhub.vn/publication/:slug/f43c618f-344f-4e53-a1ef-60fe03ac85f4) | Erst Musterlösung zeigen, dann selbst lösen; wer es schon kann, darf überspringen |
| Freies Ausprobieren ohne Anleitung | Lehrkraft erklärt direkt | *d* = −0,38 (schlechter) | [Alfieri et al. 2011](https://texasldcenter.org/?p=364) | Nie „probiert mal rum“ ohne Auftrag |
| Angeleitetes Entdecken (mit Leitfragen, Hilfen, Rückmeldung) | Andere Unterrichtsformen | *d* = 0,30 (besser) | ebd. | Entdecken ja, aber mit Auftrag und Rückmeldung |
| Lernende wählen Tempo, Reihenfolge, Inhalte frei | Programm gibt den Weg vor | *g* = 0,05 (praktisch kein Unterschied) | [Karich, Burns & Maki 2014](https://corwinvisiblelearningsolutions.sagepub.com/support/solutions/articles/7000090422-i-am-curious-about-the-poor-student-achievement-outcomes-associated-with-student-control-over-learnin) | Wahlfreiheit allein ist kein Qualitätsmerkmal |
| Lernende werden angeleitet, ihr Lernen zu planen, zu überwachen und zu reflektieren | Ohne Anleitung | *g* = 0,71 (Sekundarstufe), 0,38 (Hochschule) | Dignath & Büttner 2008; [Theobald 2021](https://www.uni-trier.de/securedl/sdl-eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpYXQiOjE3NTM2NTI4MTUsImV4cCI6MTc1Mzc0MjgxNSwidXNlciI6MCwiZ3JvdXBzIjpbMCwtMSw5ODgsOTk1XSwiZmlsZSI6ImZpbGVhZG1pbi9mYjEvcHJvZi9QU1kvU1VLL1RoZW9iYWxkXzIwMjEtMS5wZGYiLCJwYWdlIjo5NTE0OH0.rrGtHfoBsZdG8aoyxIzuKf5p-jYdLs1yv1ttKjS62ag/Theobald_2021-1.pdf) | Planen, Überprüfen und Nachdenken aktiv anleiten |
| Hilfen zur Selbstkontrolle (Lerntagebuch, Selbsteinschätzung) | Ohne | *d* = 0,42 | [Dignath & van Ewijk 2023](https://bibliographie.ub.uni-mainz.de/catalog/publication-170139) | Selbsteinschätzung vor und nach Kapiteln, Fortschrittsanzeige |
| Rückmeldung (Feedback) | Keine Rückmeldung | *d* = 0,48, stark abhängig vom Inhalt; Lob, Strafe und Belohnung wirken kaum | [Wisniewski et al. 2020](https://opus.bibliothek.uni-augsburg.de/opus4/files/71009/fpsyg-10-03087.pdf) | Rückmeldung erklärt den Fehler und zeigt den nächsten Schritt |
| Spielelemente (Gamification) | Ohne Spielelemente | *g* = 0,49 auf Wissen, 0,36 auf Motivation, 0,25 auf Verhalten; eine Rahmengeschichte und Zusammenarbeit verstärken | [Sailer & Homner 2020](https://resources.rework.com/libraries/edtech-growth/gamification-in-learning) | Story ja, Punkte und Abzeichen nur sparsam |
| Umgedrehter Unterricht (Flipped Classroom: zu Hause erarbeiten, im Unterricht anwenden) | Herkömmlicher Unterricht | *g* ≈ 0,37 (Hochschule); in IT-Fächern eher geringer | [Zusammenfassung mehrerer Übersichten](https://dro.deakin.edu.au/articles/journal_contribution/On_the_use_of_flipped_classroom_across_various_disciplines_Insights_from_a_second-order_meta-analysis/20658822); [Strelan et al.](https://able.adelaide.edu.au/education/events/list/2019/06/research-seminar-flipped-classrooms-do-they-really-work) | Pakete eignen sich gut zur Vorbereitung auf die Unterrichtsstunde |
| Mit vollständigen Notizen der Lehrkraft wiederholen | Mit eigenen Notizen wiederholen | etwa 17 % bessere Testergebnisse; halbfertige Notizen zum Ergänzen ähnlich gut | Kiewra & Benton 1987; Kiewra et al. 1995 (Übersicht Kiewra) | Muster-Merkblatt mitgeben, halb vorgefüllt zum Ergänzen (P13) |
| Concept Maps selbst erstellen | Herkömmliche Lernaktivitäten (z. B. Lesen) | g ≈ 0,74 in Klasse 9–12; Schüler profitieren mehr von teilweise vorgegebenen Maps als von leeren | Schroeder et al. 2018; Gurlitt & Renkl 2008 (zit. n. Brod 2021) | Struktur des Merkblatts vorgeben, Lernende füllen die Kernaussagen (P13) |
| Erlaubter Spickzettel in der Prüfung (selbst erstellt) | Prüfung ohne Spickzettel | kein verlässlicher Vorteil; Hinweise, dass man sich auf den Zettel verlässt statt zu lernen | Dickson & Miller 2005; Funk & Dickson 2011; Hamouda & Shaffer 2016 | Merkblatt erst nach dem Nachweis, nicht als Hilfsmittel im Nachweis (P13) |

**Was nicht belegt ist:** „Lerntypen“ (etwa der „visuelle Typ“) gibt es nach heutigem Forschungsstand nicht in einer Form, die fürs Unterrichten nützt; sie werden nicht berücksichtigt (Pashler et al. 2008; Kerres 2024). Auch „digital ist automatisch besser“ stimmt nicht: Lernen am Computer bringt über Jahrzehnte nur kleine bis mittlere Vorteile, und nur bei guter didaktischer Gestaltung (Tamim et al. 2011).

## Teil B1: Die dreizehn Prinzipien für unsere Lernpakete

Dreizehn Prinzipien reichen. Acht davon sind gut belegt und mit vertretbarem Aufwand in jedem Paket umsetzbar; sie sind verbindlich („Muss“). Dazu kommt P11, weil alle Pakete für selbstgesteuertes Lernen gebaut werden (Teil C4). Die übrigen vier sind „Soll“-Prinzipien: sinnvoll, aber abhängig von Fach, Thema und verfügbarer Zeit. Die Einschätzung von Beleglage und Aufwand stützt sich auf Teil A.

| Nr. | Prinzip | Wie gut belegt | Aufwand | So sieht es im Paket aus | Status |
| --- | --- | --- | --- | --- | --- |
| P1 | Eine realistische Handlungssituation zieht sich durchs ganze Paket | mittel bis gut | gering | Story mit Betrieb und Kundenauftrag (z. B. Korbwerk IT-Service, Bäckerei Brunner) | Muss |
| P2 | Vom Einfachen zum Schweren; die Hilfe wird schrittweise weniger | gut | mittel | Entdecken → Üben mit Tipps → Kompetenznachweis ohne Tipps | Muss |
| P3 | Erst vormachen: Musterlösungen | gut (für Einsteiger) | gering | Musterlösung je neuem Aufgabentyp, überspringbar | Muss |
| P4 | Entdecken nur mit Anleitung | gut | gering | Jede Station hat Auftrag, Leitfrage und Rückmeldung | Muss |
| P5 | Abfragen statt nochmal lesen | gut | gering | Kurz-Check (3–5 Fragen) je Kapitel, ohne Note | Muss |
| P6 | Rückmeldung, die erklärt | mittel | mittel | Begründung zu jeder Antwort, Hinweis auf typische Fehler | Muss |
| P7 | Bildschirme nach Mayer gestalten; Wichtiges und nächster Schritt sofort erkennbar | gut (für Einsteiger) | gering bis mittel | Regeln in Teil C3 | Muss |
| P8 | Ziele und Erfolgskriterien sichtbar machen; Ziele, Übungen und Prüfung passen zusammen | mittel | gering | Lernziele beim Einstieg, Auswertung je Lernziel am Ende | Muss |
| P9 | Hintergrundwissen und Bedienhinweise trennen | mittel | mittel | Hintergrund in Steckbriefen; Bedienhilfe genau dort, wo sie gebraucht wird | Soll |
| P10 | Wiederholen verteilen, Aufgaben mischen | gut | mittel | Wiederholungsfragen aus früheren Kapiteln; gemischte Aufgaben im Nachweis | Soll |
| P11 | Selbstständiges Lernen anleiten statt voraussetzen | mittel bis gut | mittel | Selbsteinschätzung, Fortschrittsanzeige, Reflexionsfrage mit Musterantwort | Muss |
| P12 | Spielelemente gezielt einsetzen | mittel | gering bis hoch | Rahmengeschichte ja; Punkte und Ranglisten nur mit klarem Zweck | Soll |
| P13 | Ergebnisse sichern: Merkblatt selbst ergänzen | mittel | gering bis mittel | Halb vorgefülltes Merkblatt je Lernziel nach dem Nachweis: aus dem Gedächtnis ergänzen, mit dem Muster vergleichen, als PDF mit Selbsttest-Fragen herunterladen | Soll |

**Zu P13: Merkblatt statt Spickzettel.** Ein fertiges Cheat Sheet zum Herunterladen sichert allein wenig: Nachlesen ist bloßes Wiederlesen, und erlaubte Spickzettel verbessern das Lernen nachweislich nicht (Teil A2). Wirksam wird das Merkblatt, wenn Lernende es nach dem Nachweis aus dem Gedächtnis ergänzen, mit dem Muster vergleichen und später zum Selbsttest nutzen. So verbindet P13 Abrufen (P5), Feedback (P6) und verteiltes Wiederholen (P10); Aufbau und Ablauf stehen in Teil C2.

**Querschnittsprinzip: iterativ entwickeln.** Kein Prinzip ersetzt den Test mit echten Schülern. Deshalb gehört eine kurze Erprobung (Teil C1, Phase 5) zu jedem Paket.

**Bezug zu den vorhandenen PING-Paketen:** „Shell Yeah!“, „Get-Going!“, „Plug & Pray“ und „Netzwerktopologien erleben“ erfüllen P1, P2, P4, P5 und P8 bereits weitgehend (Story, Tutorial → Üben → Nachweis, gestufte Tipps, Kurz-Checks, Auswertung je Lernziel). Nachschärfen lassen sich vor allem P6 (Feedback, das erklärt), P10 (verteilte Wiederholung) und P11 (Selbsteinschätzung).

## Teil B2: Was wir bewusst nicht übernehmen

Einige Ideen aus Forschung und Praxis sind entweder schlecht belegt oder für einzelne SCORM-Pakete zu aufwendig. Sie werden nicht verlangt.

| Idee | Warum nicht (oder nur vereinfacht) |
| --- | --- |
| Das vollständige 4C/ID-Planungsverfahren mit ausführlicher Aufgabenanalyse | Für ein Paket von wenigen Stunden zu aufwendig. Wir nutzen eine vereinfachte Form: Aufgaben von leicht nach schwer, gestufte Tipps, Hintergrundwissen getrennt von Bedienhilfe. |
| KI-gesteuerte Lernpfade, die sich automatisch anpassen | Kein belegter Vorteil gegenüber gut gemachten Lernprogrammen, hoher Aufwand, offene Datenschutzfragen. |
| Möglichst viel Wahlfreiheit als Qualitätsmerkmal | Freie Wahl allein bringt praktisch nichts (*g* = 0,05). Freiheit nur zusammen mit Orientierung. |
| Varianten für verschiedene „Lerntypen“ | Nicht belegt; auch Kerres (2024) stellt klar: Lerntypen gibt es nicht. |
| Erklärungen standardmäßig vorlesen lassen | Wirkt im Labor, aber im Klassenzimmer fehlen oft Kopfhörer, und Gesprochenes lässt sich schlecht überfliegen. Ton ist optional, Text immer kurz und in Schritten. |
| Hatties Rangliste als Entscheidungsgrundlage | Methodisch umstritten; nur zur Orientierung. |
| Punkte, Abzeichen, Ranglisten als Ersatz für echte Motivation | Wirkung auf das Verhalten klein und unsicher; Belohnungen können die Freude an der Sache sogar schmälern. |
| Erst ein komplettes Konzept, dann bauen | Mit KI ist ein Probelauf in Stunden gebaut; Fehler zeigen sich am Probelauf schneller als im Konzeptpapier. |

## Teil C1: Der Entwicklungsprozess in sechs Phasen

Jedes Paket durchläuft sechs Phasen; sie folgen ADDIE, laufen aber in kurzen Schleifen, und das Bauen beginnt mit einem kleinen Prototyp statt mit dem fertigen Paket. Kerres (2024) empfiehlt ein solches schrittweises Vorgehen ausdrücklich, wenn das Team klein ist, die Lehrkraft selbst inhaltlich mitarbeitet und das Ziel am Anfang noch nicht ganz klar ist: So liegt jederzeit eine nutzbare Version vor, und Wichtiges wird verlässlicher fertig als „schöne Extras“. Zu jeder Phase gehört ein konkretes Ergebnis, das ins Repository kommt.

&#91;embedded content: Entwicklungsprozess · 6 Phasen, 2 Rückschleifen\]

Durchgezogene Pfeile zeigen den Regelablauf, gestrichelte die Rücksprünge nach Erprobung und Auswertung.

1. **Klären (Analyse).** Welches Lernproblem löst das Paket, für wen, in welchem Rahmen? Ein Paket kann nur ein Lernproblem lösen; fehlen Geräte, Zeit oder Begleitung, hilft es allein nicht. Ergebnis: *Steckbrief* (eine Seite) mit Fach, Klasse, Lehrplanbezug, Vorwissen, Motivation und Erfahrung mit selbstständigem Lernen, Einsatzform (SeGeL oder klassisch), Geräten, Zeitrahmen und einer ersten Idee für die Leitsituation.
2. **Zielen (Lernziele und Nachweis zuerst).** Drei bis acht überprüfbare Lernziele formulieren, dann sofort festlegen, woran man das Können erkennt: eine Handlungsaufgabe (Nachweis Teil A) und mindestens zwei Fragen je Lernziel (Teil B). Ergebnis: *Lernziel-Nachweis-Tabelle*.
3. **Entwerfen (Drehbuch).** Leitsituation und Story, Aufgabenklassen von einfach nach komplex, Stationen, Lösungsbeispiele, gestufte Tipps. Besonders wichtig: typische Fehler und Fehlvorstellungen der Schüler sammeln, denn daraus entstehen gute Falschantworten und gutes Feedback. Ergebnis: *Drehbuch*.
4. **Prototyp bauen.** Zuerst eine einzige Station mit zwei Fragen vollständig bauen und im SCORM-Testplayer prüfen; erst wenn Optik, Bedienung und Datenübergabe stimmen, folgt der Rest. Ergebnis: *lauffähiges Paket* (ZIP).
5. **Erproben.** Selbsttest mit der Checkliste (Teil C6), Review durch eine Kollegin oder einen Kollegen (fachlich und didaktisch), Kurztest mit drei bis fünf Schülern, die laut denken, Test in der echten ByCS-Lernplattform. Ergebnis: *Änderungsliste*; zurück zu Phase 3 oder 4.
6. **Einsetzen und auswerten.** Nach dem Einsatz den Bericht aus der Lernplattform exportieren: Welche Fragen lösen fast alle falsch (unklar oder zu schwer), welche fast alle richtig (zu leicht)? Reflexionsantworten lesen. Ergebnis: *neue Version* mit Versionsnummer und kurzem Änderungsprotokoll; bei Bedarf zurück zu Phase 2.

**Rollen.** Die Lehrkraft verantwortet fachliche Richtigkeit, Lernziele und Nachweis; die KI übernimmt Programmierung, Gestaltung und Entwürfe. Alles, was die KI fachlich formuliert, wird von der Lehrkraft geprüft.

## Teil C2: Standardstruktur eines Lernpakets

Jedes Paket folgt demselben Grundmuster aus fünf Teilen. Es ist aus den PING-Paketen entstanden und setzt die Ansätze aus Teil A um: zuerst zeigen, dann begleitet üben, zuletzt selbstständig können. Einheitlichkeit hat einen eigenen Nutzen: Schüler müssen die Bedienung nur einmal lernen und können sich ganz auf den Inhalt konzentrieren.

| Teil | Zweck | Warum so (siehe Teil A) | Mindeststandard |
| --- | --- | --- | --- |
| 0 Einstieg | Orientierung und Motivation | Lernen beginnt mit einer echten Aufgabe und knüpft an Vorwissen an; Ziele sind sichtbar | Leitsituation in wenigen Sätzen; Lernziele in Schülersprache, mit kurzer Selbsteinschätzung je Ziel (bloß angezeigte Ziele werden kaum gelesen); kurzer, unbewerteter Vorwissens-Check mit Empfehlung, was übersprungen werden kann; Hinweis auf Dauer und Ablauf |
| ① Entdecken (Tutorial) | Neues verstehen | Vormachen vor Nachmachen; Bildschirme nach Mayer | Stationen bzw. Kapitel mit je einem Auftrag; Lösungsbeispiel vor eigener Aufgabe; Kurz-Check (3–5 Fragen) am Kapitelende; jede Station endet mit einem Bezug zur echten Praxis |
| ② Üben | Können aufbauen | Anwenden in wechselnden Situationen, von leicht nach schwer, mit abnehmender Hilfe | Eine Übung je Lernziel oder Kapitel, wechselnde Situationen; drei gestufte Tipps (Denkanstoß, Teillösung, Lösung); Feedback mit Begründung; Reflexionsfrage mit Musterantwort |
| ③ Kompetenznachweis | Können zeigen | Eine ganze Aufgabe ohne Hilfe lösen; Abfragen festigt das Gelernte | Teil A: Handlungsaufgabe in neuer Situation, ohne Tipps; Teil B: Fragen aus einem Pool, mindestens zwei je Lernziel, gemischte Fragetypen; Bestehensgrenze einheitlich 80 %; beliebig oft wiederholbar, bestes Ergebnis zählt (der Nachweis ist keine Prüfung im rechtlichen Sinne) |
| 4 Abschluss | Integration, Sicherung und Transfer | Gelerntes einordnen und aus dem Gedächtnis sichern; über das eigene Lernen nachdenken | Auswertung je Lernziel mit Lernempfehlung; Merkblatt zum Ergänzen und Herunterladen (P13, siehe unten); Ergebnis als PDF druckbar; Rückverweis auf passende Stationen |

**Fragetypen.** Bewährt haben sich fünf Typen: Multiple Choice, Bildfrage, Hotspot (Stelle im Bild anklicken), Zuordnen und Zahleneingabe. Falschantworten werden aus den gesammelten typischen Fehlern gebildet, nicht erfunden. Jede Frage trägt das Lernziel, zu dem sie gehört.

**Merkblatt (P13).** Nach dem Nachweis öffnet sich ein halb vorgefülltes Merkblatt, gegliedert nach Lernzielen. Lernende ergänzen die Kernaussagen ohne Zurückblättern und vergleichen dann mit dem Muster. Zum Herunterladen gibt es eine Seite als PDF: vorne die eigenen Einträge neben dem Muster, der typische Fehler mit Korrektur, ein kurzes gelöstes Beispiel und bei Abläufen das Schritt-Schema. Hinten stehen fünf Selbsttest-Fragen ohne Lösung mit dem Hinweis „In 1–2 Wochen nochmal beantworten“; die Lösungen sind im Paket. Früher freigeschaltet würde das Merkblatt nur gelesen oder zum Spickzettel für den Nachweis. Im SeGeL-Unterricht gehört es ins Lernportfolio. Wir nennen es bewusst „Merkblatt“, nicht „Cheat Sheet“.

**Größe.** Ein Paket deckt ein Thema ab, das sich in einem Satz benennen lässt. Wird das Drehbuch länger als etwa acht Stationen, lohnt es sich, zwei Pakete daraus zu machen.

Kerres (2024) nennt als Richtwert Lerneinheiten von 30 bis 45 Minuten, drei bis neun davon pro Kapitel und höchstens drei Gliederungsebenen. Wichtiger als die genaue Zahl ist, dass alle Einheiten gleich gerahmt sind: Jede beginnt mit Orientierung (worum es geht, wozu, wie lange) und endet mit einem Abschluss (Kurz-Check oder Zusammenfassung).

## Teil C3: Gestaltungsregeln

Die Regeln übersetzen die Prinzipien in überprüfbare Vorgaben; in Klammern steht der Fachbegriff, unter dem man bei Bedarf mehr dazu findet.

**Bildschirm und Darstellung (nach Mayer)**

- Alles weglassen, was nicht dem Lernziel dient: keine Deko-Bilder, keine Hintergrundmusik, keine „interessanten Nebenfakten“ (Kohärenz).
- Das Wichtige hervorheben: Schlüsselbegriffe fett, im Bild ein Pfeil oder Rahmen genau auf die Stelle, um die es geht, Zwischenüberschriften statt langer Textblöcke. Farben und Markierungen sparsam einsetzen und im ganzen Paket immer mit derselben Bedeutung (Signalisierung).
- Beschriftung direkt ans Bild, nicht in eine Legende darunter; Erklärung und zugehörige Grafik gleichzeitig sichtbar (räumliche und zeitliche Nähe; in der Lernpsychologie spricht man vom „Split-Attention-Effekt“, der geteilten Aufmerksamkeit).
- Alles, was für eine Aufgabe nötig ist, gleichzeitig sichtbar halten: Auftrag, Bild oder Terminal und Eingabe stehen auf einem Bildschirm, ohne Scrollen oder Umschalten.
- Benötigte Informationen (Steckbrief, Kundenauftrag, Tabelle) neben der Aufgabe einblenden statt in einem anderen Reiter oder Pop-up. Ist der Platz knapp, eignet sich eine aufklappbare Seitenleiste, die beim Arbeiten offen bleiben kann.
- Tipps so platzieren, dass sie nicht verdecken, worauf sie sich beziehen.
- Im Entdecken- und Übungsteil erscheint das Feedback direkt an der Antwort oder Stelle, um die es geht, nicht gesammelt an anderer Stelle.
- Auf dem Zielgerät prüfen, besonders auf dem Tablet, wo weniger Platz ist.
- Inhalte in kleine Schritte teilen, die Lernende selbst weiterklicken (Segmentierung).
- Fachbegriffe vorab kurz einführen, bevor sie in einem komplexen Ablauf auftauchen (Vortraining).
- Denselben Text nicht gleichzeitig vorlesen und anzeigen (Redundanz).
- Locker und direkt ansprechen („Du“ bzw. „Sie“ je nach Zielgruppe), nicht im Lehrbuchton (Personalisierung).
- Ein einheitliches Layout über alle Pakete einer Reihe; ein Farbschema mit einem einzigen Akzent.

Ergänzend nach Kerres (2024, Kap. 6.6):

- Das Bild vor dem zugehörigen Text zeigen; es dient als Anker für das Lesen.
- Für Einsteiger einfache Skizzen und Schemata statt Fotos oder realistischer Animationen; die Vereinfachung zeigt, worauf es ankommt.
- Einfache Grafiken mit kurzem Text erklären; sehr komplexe Abläufe dürfen zusätzlich eine gesprochene Erklärung bekommen.
- Persönliche Ansprache dosiert einsetzen und optisch vom Lernstoff abheben, z. B. als Sprechblase der Story-Figur.
- Den Bezug zu Beruf oder Alltag ausdrücklich nennen: Wozu brauche ich das?
- Signalisieren, dass die Aufgabe Anstrengung verlangt, aber zu schaffen ist; wirkt ein Angebot zu leicht, wird es nur überflogen.

**Orientierung: Wo geht es weiter?**

Signale braucht nicht nur der Lernstoff, sondern auch die Bedienung. Wer suchen muss, wo es weitergeht, verbraucht Aufmerksamkeit, die dem Lernen fehlt. Mayers Signalisierungsprinzip bezieht sich auf Inhalte; für die Bedienung stützen sich die folgenden Regeln auf Kerres' Forderung nach einfacher Navigation und auf Grundregeln guter Software-Bedienung.

- Jeder Bildschirm hat genau einen hervorgehobenen nächsten Schritt, immer an derselben Stelle (z. B. unten rechts) und immer im selben Aussehen.
- Die Akzentfarbe ist für Handlungsaufforderungen reserviert, nicht für Deko oder Überschriften.
- Was gerade zu tun ist, steht als kurzer Auftrag oben auf dem Bildschirm, nicht versteckt im Fließtext.
- Ist eine Aufgabe erledigt, zeigt das Paket das sofort sichtbar und benennt den nächsten Schritt („Geschafft! Weiter zu Station 4: Arbeitsspeicher einsetzen“).
- Eine Fortschrittsanzeige zeigt, wo man steht (Teil, Station 3 von 7) und was schon erledigt ist.
- Noch nicht verfügbare Schritte bleiben sichtbar, sind aber erkennbar ausgegraut, mit kurzem Grund.
- Anklickbares sieht anklickbar aus; Dekoratives nicht.
- Kein wichtiger Knopf liegt unbemerkt außerhalb des sichtbaren Bereichs, auch nicht auf dem Tablet.
- Prüfen mit dem Fünf-Sekunden-Test: Jemand sieht einen Bildschirm kurz an und sagt, was als Nächstes zu tun ist. Klappt das nicht, fehlt ein Signal.

**Aufgaben und Übungen**

- Jede Aufgabe beginnt mit einem Auftrag aus der Leitsituation („Mira bittet dich …“), nicht mit „Aufgabe 3“.
- Erst Lösungsbeispiel, dann ähnliche Aufgabe, dann veränderte Aufgabe (Lösungsbeispiele, Ausblenden der Hilfe).
- Situationen variieren (andere Kunden, andere Geräte, andere Daten), damit Wissen übertragbar wird (4C/ID).
- Geprüft wird das Ergebnis, nicht ein bestimmter Lösungsweg; alternative richtige Wege werden akzeptiert.

* Kurz-Checks und Übungen deutlich als „zählt nicht“ kennzeichnen: Lernaufgaben sind keine Testaufgaben.
* Multiple Choice nicht unterschätzen: Gut gebaute Fragen, bei denen mehrere Antworten richtig sein können, verlangen echtes Nachdenken und lassen sich kaum erraten. Bei Eingabefeldern alle richtigen Schreibweisen akzeptieren.
* Im Entdecken- und Übungsteil nicht hart sperren („erst weiter, wenn alles richtig ist“), sondern einen nächsten Schritt vorschlagen.

**Tipps (immer drei Stufen)**

1. Denkanstoß: Worauf solltest du achten?
2. Teillösung oder Hinweis auf die passende Stelle im Entdecken-Teil.
3. Vollständige Lösung mit kurzer Begründung.

Tipps werden aktiv angefordert und zählen im Üben nicht gegen die Lernenden; im Nachweis gibt es keine.

**Für Lernende mit Vorwissen**

Was Einsteigern hilft, kann Fortgeschrittene bremsen: Ausführliche Erklärungen, Lösungsbeispiele und kleinschrittige Anleitungen kosten sie Zeit und Aufmerksamkeit, ohne etwas Neues zu bringen, und können das Lernen sogar verschlechtern (Expertise-Umkehr-Effekt, Kalyuga 2007). Ein Paket sollte deshalb Hilfe anbieten, aber nicht aufzwingen.

- Der Vorwissens-Check im Einstieg hat Folgen: Wer gut abschneidet, bekommt eine Empfehlung, welche Stationen er überspringen oder nur überfliegen kann.
- Am Kapitelanfang gibt es einen freiwilligen Kurz-Check: Wer ihn besteht, darf das Kapitel überspringen.
- Ausführliche Erklärungen und Lösungsbeispiele sind aufklappbar, nicht Pflichtlektüre; die Kurzfassung steht sichtbar.
- Tipps gibt es nur auf Anforderung (siehe oben).
- Übersprungene Stationen bleiben jederzeit erreichbar; der Kompetenznachweis ist für alle gleich.

**Feedback**

- Zu jeder Antwort: richtig oder falsch, *warum*, und bei Fehlern, was als Nächstes hilft.
- Falsche Antworten, die auf einer typischen Fehlvorstellung beruhen, bekommen ein eigenes Feedback, das diese Fehlvorstellung anspricht.
- Kein reines Lob („Super!“) ohne Information; kein Punktestand als einziges Feedback.
- Im Nachweis wirkt sich ein Fehler, wo möglich, realistisch aus (z. B. der PC startet nicht), statt sofort erklärt zu werden; die Erklärung folgt in der Auswertung.

**Story und Spielelemente**

- Eine Rahmengeschichte mit wiederkehrenden Figuren ist erwünscht; sie trägt Motivation und Lebensweltbezug.
- Punkte, Abzeichen oder Zeitdruck nur, wenn sie eine Funktion für das Lernen haben.

* Keine Ranglisten: Fortschritt an einem sachlichen Ziel messen („7 von 9 Lernzielen erreicht“) oder an der eigenen früheren Leistung, nicht am Vergleich mit Mitschülern. Der Vergleich demotiviert schwächere Lernende und widerspricht dem Datenschutz, wenn Ergebnisse für andere sichtbar werden (Kerres 2024).

## Teil C4: SeGeL-Unterricht und klassischer Unterricht mit digitalen Medien

Die Unterscheidung ist nötig, weil im SeGeL-Unterricht das Paket die Aufgaben der Lehrkraft teilweise übernehmen muss: Orientierung, Strukturierung, Rückmeldung und Anleitung zur Selbststeuerung. Im klassischen Unterricht übernimmt die Lehrkraft diese Rollen selbst, und das Paket ist ein Baustein. Die Gestaltungsprinzipien P1–P8 gelten in beiden Fällen gleich.

Die Begründung liefert die Forschung zur Selbststeuerung: Mehr Wahlfreiheit allein verbessert das Lernen nicht (*g* = 0,05), angeleitetes selbstreguliertes Lernen dagegen deutlich (*g* = 0,71 in der Sekundarstufe), ebenso Werkzeuge zur Selbstüberwachung (*d* = 0,42). Ein SeGeL-Paket muss Selbststeuerung also *unterstützen*, nicht nur *erlauben*.

Kerres (2024) bestätigt das aus der Forschung zum digitalen Lernen: Wer allein am Computer lernt, kommt schneller voran, bricht aber auch häufiger ab, und Phasen mit Lehrkraft verbessern die Ergebnisse. Die Lernbegleitung durch die Lehrkraft bleibt deshalb auch im SeGeL-Unterricht ein fester Bestandteil.

| Merkmal | SeGeL (selbstgesteuert) | Klassisch (lehrkraftgeleitet) |
| --- | --- | --- |
| Rolle des Pakets | Trägt die gesamte Lerneinheit von Einstieg bis Nachweis | Baustein für eine Phase: Erarbeitung, Übung, Simulation oder Vorbereitung (Flipped) |
| Zeitstruktur | Kein 45/90-Minuten-Raster; Fortschritt wird gespeichert, Wiedereinstieg jederzeit | Abschnitte von etwa 20–30 Minuten, die in eine Stunde passen; Stationen einzeln aufrufbar |
| Einstieg | Im Paket: Leitsituation, Lernziele, Vorwissens-Check, Hinweis zum Vorgehen | Durch die Lehrkraft; das Paket kann den Einstieg kurz halten |
| Steuerung | Empfohlener Pfad mit Freiheit beim Tempo; Springen möglich, aber mit Hinweis, was vorausgesetzt wird | Lehrkraft gibt Abschnitte frei bzw. vor |
| Hilfe | Drei gestufte Tipps, Lösungsbeispiele, Glossar und Steckbriefe immer erreichbar; Hinweis, wann man die Lehrkraft fragen sollte | Tipps im Paket; vertiefende Hilfe durch Lehrkraft und Mitschüler |
| Selbstregulation | Verbindlich: Lernziele mit Selbsteinschätzung vorher/nachher, Fortschrittsanzeige, Reflexionsfragen mit Musterantwort, Lernempfehlung nach dem Nachweis | Optional; Reflexion und Sicherung im Plenum |
| Nachweis | Im Paket, mit Bestehensgrenze und Wertung in der Lernplattform; mehrere Versuche, bestes Ergebnis zählt | Oft formativ (ohne Wertung) oder ganz ausgelagert in eine Lernzielkontrolle der Lehrkraft |
| Rolle der Lehrkraft | Lernbegleitung: Auswertung der Daten, gezielte Gespräche mit Lernenden, die hängen | Unterrichtsleitung: Einführung, Moderation, Sicherung der Ergebnisse |
| Daten für die Lehrkraft | Wichtig: Ergebnisse je Lernziel und je Frage, Bearbeitungsstand | Hilfreich, aber nicht zwingend |
| Lebensweltbezug (K+5) | Trägt die Motivation über längere Zeit: echte Aufgaben, Bezug zu eigenen Geräten und Medien | Kann von der Lehrkraft im Unterrichtsgespräch hergestellt werden |

**Festlegung:** Pakete grundsätzlich für SeGeL bauen und so gliedern, dass einzelne Stationen auch im klassischen Unterricht einsetzbar sind. Umgekehrt (ein klassisches Paket nachträglich „SeGeL-tauglich“ machen) ist deutlich aufwendiger. Das gilt für alle Fächer, auch dort, wo es noch keine SeGeL-Kurse gibt.

**Wie offen darf ein Paket sein?** Kerres (2024, Kap. 10.5) nennt Merkmale, nach denen sich entscheiden lässt, ob ein fester Lernweg oder freies Erkunden besser passt:

| Eher fester Lernweg, wenn … | Eher freies Erkunden, wenn … |
| --- | --- |
| der Stoff aufeinander aufbaut | die Themen nebeneinander stehen |
| das Paket Teil eines Lehrgangs mit Prüfung ist | es um freiwilliges Weiterlernen geht |
| die Klasse ähnliches Vorwissen hat | die Lernenden sehr unterschiedlich sind |
| die Lernenden selbstständiges Lernen wenig gewohnt sind | sie gern und geübt selbstständig lernen |
| die Motivation vor allem von außen kommt (Note, Prüfung) | eigenes Interesse trägt |
| das Vorwissen gering ist | die Grundbegriffe schon sitzen |

Für die meisten Berufsschulklassen spricht das für einen empfohlenen Weg mit Freiheit beim Tempo, wie oben beschrieben. Mit wachsender Erfahrung der Lernenden kann ein Paket mehr Freiheit lassen.

## Teil C5: Technische Mindeststandards

Die Standards stammen aus den bisherigen PING-Paketen und den Tests in der ByCS-Lernplattform; sie sorgen dafür, dass jedes Paket dort läuft und die Ergebnisse ankommen.

| Bereich | Standard |
| --- | --- |
| Format | SCORM 1.2, eine ZIP-Datei mit `imsmanifest.xml` und `index.html`; alle Schriften, Skripte und Bilder im Paket (keine externen Server); unter 512 MB (ByCS-Grenze), in der Praxis meist unter 1 MB |
| Datenübergabe | `cmi.core.score.raw` = bestes Ergebnis (0–100), `lesson_status` passed/failed, `masteryscore` im Manifest = 80; Fortschritt in `suspend_data`; Name aus `cmi.core.student_name` vorbelegen; optional `cmi.objectives` je Lernziel und `cmi.interactions` je Frage |
| Ohne Lernplattform | Paket läuft auch lokal; Fortschritt dann nur im Browser, mit Hinweis |
| Geräte | Zielgerät im Steckbrief festlegen (PC, Tablet oder beides); bei Tablets: Antippen statt nur Ziehen |
| Barrierefreiheit | Bedienung per Tastatur, ausreichender Kontrast, Alternativtexte für Bilder, Schrift skalierbar, Information nie nur über Farbe |
| Datenschutz | Keine Daten an Dritte, keine Tracking-Dienste, keine Konten außerhalb der Lernplattform; nur die SCORM-Felder, die für Lernen und Bewertung nötig sind |
| Ergebnis | Auswertung je Lernziel, druckbar als PDF |
| Qualitätssicherung | Test im SCORM-1.2-Testplayer; automatisierter Test, dass alle Aufgaben mit der Musterlösung lösbar sind; Test in der echten ByCS-Lernplattform vor dem ersten Einsatz |
| Ablage | Quellcode und ZIP im gemeinsamen Repository, einheitliche Benennung, Versionsnummer und Änderungsprotokoll |
| Lizenz | Lizenz festlegen (z. B. CC BY-SA), damit Kollegen Pakete anpassen und weitergeben dürfen |
| Merkblatt (P13) | Wird im Browser als druckbare Seite erzeugt und als PDF gespeichert, ohne Server. Einträge nicht dauerhaft in suspend\_data ablegen (SCORM 1.2: höchstens 4096 Zeichen); das Merkblatt braucht keinen Namen. |

Kolleginnen und Kollegen ohne Programmiererfahrung müssen diese Details nicht selbst umsetzen; das Skill-Set (Teil C7) baut sie automatisch ein. Sie sollten aber wissen, dass es sie gibt, um die Checkliste abhaken zu können.

## Teil C6: Qualitätscheckliste für das Review

Ein Paket ist einsatzbereit, wenn alle Muss-Punkte erfüllt sind; die Checkliste nutzen Autorin bzw. Autor zum Selbsttest und eine zweite Lehrkraft beim Review.

**Didaktik (Muss)**

- [ ] Es gibt eine authentische Leitsituation, die sich durch das ganze Paket zieht. (P1)
- [ ] Lernziele sind überprüfbar formuliert und beim Einstieg sichtbar. (P8)
- [ ] Jedes Lernziel wird im Nachweis mit mindestens zwei Fragen oder Prüfpunkten erfasst. (P8)
- [ ] Die Hilfe nimmt von Entdecken über Üben bis Nachweis ab. (P2)
- [ ] Vor jedem neuen Aufgabentyp gibt es ein Lösungsbeispiel, das Könner überspringen können. (P3)
- [ ] Jede Entdecken-Station hat einen klaren Auftrag und eine Rückmeldung. (P4)
- [ ] Jedes Kapitel endet mit einem unbewerteten Kurz-Check. (P5)
- [ ] Jede Antwort bekommt ein Feedback mit Begründung; typische Fehler haben eigenes Feedback. (P6)
- [ ] Jede Übung hat drei gestufte Tipps. (P2)
- [ ] Selbsteinschätzung vorher und nachher, Fortschrittsanzeige. (P11)

**Gestaltung (Muss)**

- [ ] Keine Deko-Elemente ohne Funktion für das Lernziel. (P7)
- [ ] Text und zugehörige Grafik stehen zusammen; Beschriftungen direkt im Bild. (P7)
- [ ] Inhalte sind in kurze Schritte geteilt; kein Bildschirm braucht Scrollen, um eine Aufgabe zu verstehen. (P7)
- [ ] Fachbegriffe werden eingeführt, bevor sie gebraucht werden. (P7)
- [ ] Layout und Bedienung entsprechen der Paketreihe.

* [ ] Wichtiges ist hervorgehoben; Farben und Markierungen haben im ganzen Paket dieselbe Bedeutung. (P7)
* [ ] Auf jedem Bildschirm ist ohne Suchen erkennbar, was zu tun ist und wo es weitergeht; Fünf-Sekunden-Test bestanden, auch auf dem Tablet. (P7)

- [ ] Alles, was für eine Aufgabe nötig ist, ist gleichzeitig sichtbar, ohne Scrollen, Umschalten oder Pop-up; Tipps und Feedback verdecken nichts und stehen nah an der Stelle, auf die sie sich beziehen. (P7)

* [ ] Lernende mit Vorwissen kommen zügig durch: Vorwissens-Check mit Empfehlung, überspringbare Kapitel und Beispiele, aufklappbare Erklärungen. (P3)

**Fachlich (Muss)**

- [ ] Eine Fachlehrkraft hat alle Inhalte, Musterlösungen und Fragen geprüft.
- [ ] Alle Aufgaben sind mit der Musterlösung lösbar; alternative richtige Lösungswege werden akzeptiert.

**Technik (Muss)**

- [ ] Paket läuft im Testplayer und in der ByCS-Lernplattform; Punktzahl und Status kommen an.
- [ ] Wiedereinstieg nach Abbruch funktioniert.
- [ ] Tastaturbedienung, Kontrast und Alternativtexte geprüft.
- [ ] Keine externen Verbindungen, keine personenbezogenen Daten außerhalb der Lernplattform.

**Soll**

- [ ] Wiederholungsfragen zu früheren Kapiteln sind eingestreut. (P10)
- [ ] Trennung von Hintergrundwissen (Steckbrief, Glossar) und Bedienhilfe am Ort der Aufgabe. (P9)
- [ ] Mit drei bis fünf Schülern erprobt; Änderungen eingearbeitet.
- [ ] Am Ende gibt es ein halb vorgefülltes Merkblatt zum Ergänzen und Herunterladen, freigeschaltet erst nach dem Nachweis, mit Selbsttest-Fragen zum späteren Wiederholen. (P13)

## Teil C7: Der Leitfaden als Skill-Set

Der Leitfaden wird zusätzlich als Skill-Set für Claude bereitgestellt: Jede Phase aus Teil C1 bekommt einen Skill, der die Lehrkraft im Gespräch durch die Phase führt und die Prinzipien automatisch anwendet. So müssen Einsteiger den Leitfaden nicht auswendig kennen, um ihn einzuhalten.

| Skill | Phase | Was er tut | Die Lehrkraft liefert | Ergebnis |
| --- | --- | --- | --- | --- |
| `lernpaket-planen` | 1–3 | Führt ein Interview (Fach, Klasse, Vorwissen, Einsatzform, Leitsituation), formuliert Lernziele, schlägt Nachweis und Drehbuch vor; fragt gezielt nach typischen Schülerfehlern | Fachwissen, Unterrichtserfahrung, Entscheidungen | Steckbrief, Lernziel-Nachweis-Tabelle, Drehbuch |
| `fragenpool-erstellen` | 2–3 | Erzeugt Fragen je Lernziel in den fünf Fragetypen, Falschantworten aus typischen Fehlern, Feedback zu jeder Antwort | Lernziele, typische Fehler, Prüfung der Fragen | Fragenpool (geprüft) |
| `lernpaket-bauen` | 4 | Baut das SCORM-Paket im einheitlichen Design mit fertigen Bausteinen (Station, Kurz-Check, Übung mit Tipps, Nachweis, Auswertung, PDF); erst Prototyp, dann Rest | Drehbuch, Rückmeldung zum Prototyp | ZIP-Datei |
| `lernpaket-pruefen` | 5 | Geht die Checkliste aus Teil C6 durch, testet die Datenübergabe, prüft Lösbarkeit und Barrierefreiheit, erstellt einen Review-Bericht | Paket, Ergebnis des Schülertests | Prüfbericht mit Ampel |
| `lernpaket-auswerten` | 6 | Liest den Berichtsexport aus der Lernplattform, zeigt schwierige und zu leichte Fragen, fasst Reflexionsantworten zusammen | Berichtsexport (ohne Namen) | Auswertung und Änderungsvorschläge |

Gemeinsame Grundlage aller Skills ist eine Referenzdatei mit den dreizehn Prinzipien und den Regeln aus C3–C5, damit alle Skills dieselben Maßstäbe anlegen. Der vorhandene Skill `bycs-lernmodul` (Look und Einbettung) geht in `lernpaket-bauen` auf.

**So arbeitet eine Kollegin ohne Programmiererfahrung (Beispiel)**

1. Sie öffnet Claude im gemeinsamen Projekt und schreibt: „Ich möchte ein Lernpaket zum Thema Dreisatz für die Berufsintegrationsklasse planen.“ Der Planungs-Skill startet und stellt Fragen.
2. Sie beantwortet die Fragen, korrigiert die vorgeschlagenen Lernziele und nennt typische Fehler ihrer Schüler. Claude legt Steckbrief und Drehbuch als Dokument ab.
3. Sie sagt: „Bau mir den Prototyp.“ Claude baut eine Station und zwei Fragen; sie probiert sie im Browser aus und gibt Rückmeldung in normaler Sprache.
4. Nach dem Ausbau lässt sie das Paket prüfen, gibt es einer Kollegin zum Review und testet es mit einigen Schülern.
5. Sie lädt die ZIP-Datei als Aktivität „Lernpaket“ in die ByCS-Lernplattform hoch.
6. Nach dem Einsatz exportiert sie den Bericht und lässt ihn auswerten.

**Grenzen, die im Leitfaden klar stehen müssen:** Die KI kann fachliche Fehler machen; jede Musterlösung und jede Frage wird von einer Lehrkraft geprüft. Schülerdaten werden nur ohne Namen an die KI gegeben. Die Entscheidung über Lernziele und Bewertung bleibt bei der Lehrkraft.

**Bereitstellung in zwei Formen:** (1) dieses Dokument als Langfassung plus eine zweiseitige Kurzfassung mit den zehn Kernsätzen und der Checkliste; (2) das Skill-Set als Paket für Claude, zusammen mit einer bebilderten Kurzanleitung zur Einrichtung.

## Offene Punkte und nächste Schritte

- [ ] Leitfaden mit zwei bis drei Kolleginnen und Kollegen aus anderen Fächern gegenlesen: Passt das Grundmuster auch für wissenslastige Fächer (z. B. Deutsch, Wirtschaft)? Dort trägt vermutlich Abrufen und Verteilen (P5, P10) mehr als die Handlungsaufgabe.
- [ ] Test eines PING-Pakets in der echten ByCS-Lernplattform abschließen (steht für „Plug & Pray“ und „Netzwerktopologien erleben“ noch aus).
- [ ] Vorhandene PING-Pakete gegen die Checkliste prüfen und P6, P10, P11 nachschärfen; bei „Plug & Pray“ zuerst die Orientierung („Wo geht es weiter?“) verbessern.
- [ ] Kurzfassung (zwei Seiten) und bebilderte Einrichtungsanleitung erstellen.
- [ ] Skill-Set bauen, zuerst `lernpaket-planen` und `lernpaket-pruefen`, weil sie unabhängig von der Technik den größten Nutzen bringen.
- [ ] Lizenz und Ablage für Pakete anderer Fächer klären (Repository, SCORM Hub).
- [x] Kapitel aus Kerres (2024) im Original prüfen und die Zusammenfassung in Teil A1 abgleichen.
- [ ] Bestehensgrenze der vorhandenen PING-Pakete von 70 % bzw. 60 % auf 80 % umstellen (Prüfen, ob die Fragen dazu passen).

## Literatur

Links führen zum Volltext oder zu einer Zusammenfassung, die für diesen Entwurf eingesehen wurde; Titel ohne Link sind Standardliteratur oder Sekundärangaben. Vor einer Veröffentlichung sollten die Zahlen am Original geprüft werden.

- Adesope, O. O., Trevisan, D. A. & Sundararajan, N. (2017). [Rethinking the use of tests: A meta-analysis of practice testing](https://education.wsu.edu/documents/2018/01/rethinking-use-tests.pdf/). *Review of Educational Research, 87*(3), 659–701.
- Alfieri, L., Brooks, P. J., Aldrich, N. J. & Tenenbaum, H. R. (2011). [Does discovery-based instruction enhance learning?](https://www.fisme.science.uu.nl/publicaties/literatuur/2011_alfieri_discovery_learning.pdf) *Journal of Educational Psychology, 103*(1), 1–18.
- Barbieri, C. A., Miller-Cotto, D., Clerjuste, S. N. & Chawla, K. (2023). [A meta-analysis of the worked examples effect on mathematics performance](https://oaks.kent.edu/hcri/meta-analysis-worked-examples-effect-mathematics-performance). *Educational Psychology Review, 35*.
- Brod, G. (2021). Generative learning: Which strategies for what age? Educational Psychology Review, 33, 1295–1318. ([Volltext](https://www.pedocs.de/volltexte/2021/23704/pdf/EPR_2021_4_Brod_Generative_Learning_A.pdf))
- Brunmair, M. & Richter, T. (2019). [Similarity matters: A meta-analysis of interleaved learning and its moderators](https://www.psychologie.uni-wuerzburg.de/fileadmin/06020400/2019/Brunmair_Richter_in_press__2019_META-ANALYSIS_OF_INTERLEAVED_LEARNING.pdf). *Psychological Bulletin, 145*(11), 1029–1052.
- Collins, A., Brown, J. S. & Newman, S. E. (1989). Cognitive apprenticeship: Teaching the crafts of reading, writing, and mathematics. In L. B. Resnick (Hrsg.), *Knowing, learning, and instruction* (S. 453–494). Erlbaum.
- Costa, J. M., Miranda, G. & Melo, M. (2022). [The four-component instructional design model (4C/ID): A meta-analysis on use and effect](https://repositorio.iscte-iul.pt/bitstream/10071/23015/1/article_81383.pdf). *Learning Environments Research, 25*(2), 445–463.
- Dickson, K. L. & Miller, M. D. (2005). Authorized crib cards do not improve exam performance. Teaching of Psychology, 32(4), 230–233.
- Dignath, C. & Büttner, G. (2008). Components of fostering self-regulated learning among students. *Metacognition and Learning, 3*, 231–264.
- Dignath, C. & van Ewijk, R. (2023). [Let learners monitor the learning content and their learning behavior!](https://bibliographie.ub.uni-mainz.de/catalog/publication-170139) *Educational Psychology Review, 35*, Art. 62.
- Frick, T. W., Chadha, R., Watson, C. & Zlatkovska, E. (2010). [Improving course evaluations to improve instruction and complex learning in higher education](https://bio.tedfrick.me/etrd/ETRD2010ImprovingCourseEvaluations.pdf). *ETR&D, 58*(2), 115–136.
- Funk, S. C. & Dickson, K. L. (2011). Crib card use during tests: Helpful or a crutch? Teaching of Psychology, 38(2), 114–117. ([Abstract](https://experts.nau.edu/en/publications/crib-card-use-during-testshelpful-or-a-crutch/))
- Gurlitt, J. & Renkl, A. (2008). Are high-coherent concept maps better for prior knowledge activation? Differential effects of concept mapping tasks on high school vs. university students. Journal of Computer Assisted Learning, 24(5), 407–419.
- Hamouda, S. & Shaffer, C. A. (2016). Crib sheets and exam performance in a data structures course. Computer Science Education. ([Volltext](https://people.cs.vt.edu/shaffer/Papers/Crib_Sheets2016.pdf))
- Hattie, J. (2023). *Visible learning: The sequel*. Routledge. ([Rezension](https://jle.hse.ru/article/download/27821/23245))
- Iterbeke, K., De Witte, K. & Schelfhout, W. (2021). The effects of computer-assisted adaptive instruction and elaborated feedback on learning outcomes. A randomized control trial. *Computers in Human Behavior, 120*, 106666.
- Kalyuga, S. (2007). Expertise reversal effect and its implications for learner-tailored instruction. *Educational Psychology Review, 19*, 509–539.
- Karich, A. C., Burns, M. K. & Maki, K. E. (2014). Updated meta-analysis of learner control within educational technology. *Review of Educational Research, 84*(3), 392–410. ([Kurzreview Clearing House Unterricht](https://www.clearinghouse.edu.tum.de/wp-content/uploads/2017/07/CHU_KR_8_Karich_2014_Lernersteuerung-Software.pdf))
- Kerres, M. (2024). [*Mediendidaktik: Lernen in der digitalen Welt*](https://www.degruyterbrill.com/document/doi/10.1515/9783111201078/html) (6. Aufl.). De Gruyter Oldenbourg. Genutzt v. a. Kap. 4 (Wirksamkeit digitalen Lernens), 6.6 (Gestaltung), 8.3 und 8.6 (Instruktionsdesign, Vorgehen), 9.2 (Bildungsproblem), 10.2–10.5 (Steuerung, 3-2-1-Modell, adaptive Systeme, Kriterien für die Methode), 11.6 (Spielelemente), 12.6 (Struktur, Lernaufgaben) und 14 (Planungsleitfaden).
- Kerres, M., Buntins, K., Buchner, J., Drachsler, H. & Zawacki-Richter, O. (o. J., Preprint). [Lernpfade in adaptiven und künstlich-intelligenten Lernprogrammen](https://learninglab.uni-due.de/sites/default/files/KI-Bildung-Sammelband-V1.4_1.pdf). In C. de Witt u. a. (Hrsg.), *Künstliche Intelligenz in der Bildung*. Springer VS.
- Kiewra, K. A. (o. J.). Note this: How to improve student note taking. University of Nebraska–Lincoln. Darin u. a. Kiewra & Benton (1987) und Kiewra et al. (1995). ([Übersicht](https://cehs.unl.edu/images/cehs/soar/articles/Note%20This.%20How%20to%20Improve%20Student%20Note%20Taking.pdf))
- Mayer, R. E. (2017). Using multimedia for e-learning. *Journal of Computer Assisted Learning, 33*(5), 403–423. ([Abstract mit Effektstärken](https://www.synapsesocial.com/es/papers/69dbbbdacebd566818835a69))
- Mayer, R. E. (2021). *Multimedia learning* (3. Aufl.). Cambridge University Press. ([Mayer: The future of multimedia learning](https://edtechbooks.org/docx/10349))
- Merrill, M. D. (2002). [First principles of instruction](https://classroom.eumetsat.int/mod/resource/view.php?id=14612). *ETR&D, 50*(3), 43–59.
- Pashler, H., McDaniel, M., Rohrer, D. & Bjork, R. (2008). Learning styles: Concepts and evidence. *Psychological Science in the Public Interest, 9*(3), 105–119.
- Ropovik, I., Adamkovic, M. & Greger, D. (2021). [Neglect of publication bias compromises meta-analyses of educational research](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC8174709/). *PLOS ONE, 16*(6).
- Sailer, M. & Homner, L. (2020). [The gamification of learning: A meta-analysis](https://opus.bibliothek.uni-augsburg.de/opus4/frontdoor/index/index/docId/109056). *Educational Psychology Review, 32*(1), 77–112.
- Sarfo, F. K. & Elen, J. (2007). Developing technical expertise in secondary technical schools: The effect of 4C/ID learning environments. *Learning Environments Research, 10*, 207–221.
- Schroeder, N. L., Nesbit, J. C., Anguiano, C. J. & Adesope, O. O. (2018). Studying and constructing concept maps: A meta-analysis. Educational Psychology Review, 30(2), 431–455.
- Shah, A. & Soosai Raj, A. G. (2024). [A review of cognitive apprenticeship methods in computing education research](https://par.nsf.gov//servlets/purl/10491208). *SIGCSE 2024*.
- Tamim, R. M. u. a. (2011). What forty years of research says about the impact of technology on learning. *Review of Educational Research, 81*(1), 4–28.
- Theobald, M. (2021). [Self-regulated learning training programs enhance university students' academic performance, self-regulated learning strategies, and motivation](https://www.uni-trier.de/securedl/sdl-eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpYXQiOjE3NTM2NTI4MTUsImV4cCI6MTc1Mzc0MjgxNSwidXNlciI6MCwiZ3JvdXBzIjpbMCwtMSw5ODgsOTk1XSwiZmlsZSI6ImZpbGVhZG1pbi9mYjEvcHJvZi9QU1kvU1VLL1RoZW9iYWxkXzIwMjEtMS5wZGYiLCJwYWdlIjo5NTE0OH0.rrGtHfoBsZdG8aoyxIzuKf5p-jYdLs1yv1ttKjS62ag/Theobald_2021-1.pdf). *Contemporary Educational Psychology*.
- van Merriënboer, J. J. G., Clark, R. E. & de Croock, M. B. M. (2002). [Blueprints for complex learning: The 4C/ID-model](https://link.springer.com/doi/10.1007/BF02504993). *ETR&D, 50*(2), 39–64.
- van Merriënboer, J. J. G. & Kirschner, P. A. (2017). *Ten steps to complex learning* (3. Aufl.). Routledge.
- Wisniewski, B., Zierer, K. & Hattie, J. (2020). [The power of feedback revisited: A meta-analysis of educational feedback research](https://pmc.ncbi.nlm.nih.gov/articles/PMC6987456). *Frontiers in Psychology, 10*, 3087.
