/* Slides mostrados enquanto o Kalango OS é copiado para o disco. */
import QtQuick 2.0;
import calamares.slideshow 1.0;

Presentation
{
    id: presentation

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
            text: "Instalando o Kalango OS.<br/>O resto é automático e leva alguns minutos."
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
            text: "<b>Super</b> abre o menu de apps · <b>Super+Enter</b> abre o terminal<br/>" +
                  "<b>Super+L</b> bloqueia a tela · <b>Alt+Tab</b> troca de janela"
            color: "#e6edf3"
            font.pixelSize: 16
            wrapMode: Text.WordWrap
            width: 600
            horizontalAlignment: Text.Center
        }
    }
}
