const wsUri = "ws://localhost:3000/ws";
const websocket = new WebSocket(wsUri);

websocket.addEventListener("open", () => {
  console.log("CONNECTED");
});

websocket.addEventListener("message", (e) => {
  console.log(`RECEIVED: ${e.data}`);
});

async function sendVideo(index) {
    let packet = osc.writePacket({
        address: "/video",
        args: [
            { type: "i", value: index }
        ]
    })
    websocket.send(packet)
}

async function loadVideos() {
  try {
    const response = await fetch("videos");
    if (!response.ok) {
      throw new Error(`HTTP error! Status: ${response.status}`);
    }
    const data = await response.json();
    console.log(data);
    return data;
  } catch (error) {
    console.error('Failed to load JSON:', error);
  }
}

function createVideoButtons(videoNames) {
    let videoContainer = document.getElementById("videos");
    videoNames.forEach((name, index) => {
        let div = document.createElement("div")
        div.dataset.video_idx = index;
        let img = document.createElement("img")
        img.src = "thumbnail/" + index;
        let span = document.createElement("span")
        span.innerText = name;
        div.appendChild(span)
        div.appendChild(img)
        videoContainer.appendChild(div)
        div.onclick = () => {
            sendVideo(index);
        };
    });
}

async function setup() {
    let videos = await loadVideos();
    createVideoButtons(videos);
}


setup();
