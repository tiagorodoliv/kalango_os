/* Slides mostrados enquanto o Kalango OS é copiado para o disco. */
import QtQuick 2.0;
import calamares.slideshow 1.0;

Presentation
{
    id: presentation

    // Textos em português quando o instalador está em português; senão, inglês.
    readonly property bool pt: Qt.uiLanguage.substring(0, 2) === "pt" || Qt.locale().name.substring(0, 2) === "pt"

    // Fundo da área inteira do slideshow (sem ele sobra uma borda branca).
    Rectangle {
        anchors.fill: parent
        color: "#07090d"
        z: -1
    }

    Timer {
        interval: 15000
        repeat: true
        onTriggered: presentation.goToNextSlide()
    }

    Slide {
        Rectangle { anchors.fill: parent; color: "#07090d" }
        Image {
            id: logo
            source: "kalango-logo.png"
            width: 420; height: 140
            fillMode: Image.PreserveAspectFit
            anchors.centerIn: parent
            anchors.verticalCenterOffset: -40
        }
        Text {
            anchors.horizontalCenter: logo.horizontalCenter
            anchors.top: logo.bottom
            anchors.topMargin: 24
            text: presentation.pt
                ? "Instalando o Kalango OS.<br/>O resto é automático e leva alguns minutos."
                : "Installing Kalango OS.<br/>The rest is automatic and takes a few minutes."
            color: "#e6edf3"
            font.pixelSize: 16
            wrapMode: Text.WordWrap
            width: 600
            horizontalAlignment: Text.Center
        }
    }

    Slide {
        Rectangle { anchors.fill: parent; color: "#07090d" }
        Text {
            anchors.centerIn: parent
            text: presentation.pt
                ? "<b>Super</b> abre o menu de apps · <b>Super+Enter</b> abre o terminal<br/>" +
                  "<b>Super+L</b> bloqueia a tela · <b>Alt+Tab</b> troca de janela"
                : "<b>Super</b> opens the app menu · <b>Super+Enter</b> opens the terminal<br/>" +
                  "<b>Super+L</b> locks the screen · <b>Alt+Tab</b> switches windows"
            color: "#e6edf3"
            font.pixelSize: 16
            wrapMode: Text.WordWrap
            width: 600
            horizontalAlignment: Text.Center
        }
    }
}
