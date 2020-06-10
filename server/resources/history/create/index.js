let socket = new WebSocket("ws://ethan.ws/history");
socket.onclose = () => console.log("socket closed");
socket.onmessage = msg => {
    let data = JSON.parse(msg.data);
    console.log("received " + data.kind);

    if (data.kind == "createSuccess") {
        document.getElementById("gameId").innerText = "Game ID: " + data.gameId;
        document.getElementById("hostName").innerText = data.hostName + "'s Lobby";

        document.getElementById("createMenu").style.display = "none";
        document.getElementById("lobby").style.display = "block";
    

    } else if (data.kind == "createFailed") {
        document.getElementById("errorLabel").innerText = data.message;

    } else if (data.kind == "refreshLobby") {
        document.getElementById("members").value = data.users.join("\n");
    } else if (data.kind == "initialStuff") {
        document.getElementById("lobby").style.display = "none";
        document.getElementById("game").style.display = "block";



    }
};

function nextQuestionHandler() {
    socket.send(JSON.stringify({
        kind: "nextQuestion"
    }));
}

function createGameButtonHandler() {
    let gameKindSelector = document.getElementById("gameKindSelect");
    let gameKind = gameKindSelector.options[gameKindSelector.selectedIndex].value;

    socket.send(JSON.stringify({
        kind: "create",
        username: document.getElementById("username").value,
        settings: {
            startSection: document.getElementById("startSection").value,
            endSection: document.getElementById("endSection").value,
            gameKind: gameKind,
       },
    }));
}

function startGameButtonHandler() {
    socket.send(JSON.stringify({
        kind: "start",
    }));
}