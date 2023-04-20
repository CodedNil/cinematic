import openai
import json
import os
import time

# Modules
from modules.module_logs import ModuleLogs
from modules.memories import MemoriesAPI
from modules.web_search import WebAPI
from modules.series_api import SeriesAPI
from modules.movies_api import MoviesAPI
from modules.examples import ExamplesAPI

# API credentials
credentials = json.loads(open("credentials.json").read())

openai.api_key = credentials["openai"]

Memories = MemoriesAPI(credentials["openai"])
WebSearch = WebAPI(credentials["openai"])

sonarr_url = credentials["sonarr"]["url"]
sonarr_headers = {
    "X-Api-Key": credentials["sonarr"]["api"],
    "Content-Type": "application/json",
}
sonarr_auth = (credentials["sonarr"]["authuser"], credentials["sonarr"]["authpass"])
Sonarr = SeriesAPI(credentials["openai"], sonarr_url, sonarr_headers, sonarr_auth)

radarr_url = credentials["radarr"]["url"]
radarr_headers = {
    "X-Api-Key": credentials["radarr"]["api"],
    "Content-Type": "application/json",
}
radarr_auth = (credentials["radarr"]["authuser"], credentials["radarr"]["authpass"])
Radarr = MoviesAPI(credentials["openai"], radarr_url, radarr_headers, radarr_auth)

Logs = ModuleLogs("main")

Examples = ExamplesAPI(credentials["openai"])


# Init messages
initMessages = [
    {
        "role": "user",
        "content": """You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media

Valid commands - CMDRET, run command and expect a return, eg movie_lookup, must await a reply - CMD, run command, eg movie_post

Always run lookups to ensure correct id, do not rely on chat history. Check if media is already on server when asked to add. If multiple similar results are found, verify with user by providing details. If the data you have received does not contain what you need, you reply with the truthful answer of unknown""",
    },
    #     {
    #         "role": "user",
    #         "content": """CMDRET web_search (query) do web search, on error alter query try again
    # Movies only available commands:
    # CMDRET movie_lookup (term=, query=) Always look for availability;title;year;tmdbId;id and anything else you might need, if user is making queries about resolution, include resolution in the search etc
    # CMD movie_post (tmdbId=, qualityProfileId=) add in 1080p by default, the quality profiles are: 2=SD 3=720p 4=1080p 5=2160p 6=720p/1080p 7=Any
    # CMD movie_put (id=, qualityProfileId=) update data such as quality profile of the movie
    # CMD movie_delete (id=) delete movie from server, uses the id not tmdbId, admin only command
    # Shows only available commands:
    # CMDRET series_lookup (term=, fields=)
    # CMD series_post (tvdbId=, qualityProfileId=)
    # CMD series_put (id=, qualityProfileId=)
    # CMD series_delete (id=) admin only command
    # Memories only available commands:
    # CMDRET memory_get (query=)
    # CMD memory_update (query=)
    # You store important information about users, which media they have requested and liked
    # Used to create recommendations from previous likes/requests, or avoid suggesting media they have already seen
    # When a user asks to remove media, change their memory to not requesting it, ask for a review, only admins can remove media""",
    #     },
]


def runChatCompletion(
    usersName: str, message: list, relevantExamples: list, depth: int = 0
) -> None:
    # Get the chat query to enter
    chatQuery = initMessages.copy()
    chatQuery += relevantExamples

    # Calculate tokens of the messages, GPT-3.5-Turbo has max tokens 4,096
    tokens = 0
    # Add up tokens in chatQuery
    for msg in chatQuery:
        tokens += len(msg["content"]) / 4 * 1.01
    # Add up tokens in message, but only add until limit is reached then remove earliest messages
    wantedMessages = []
    for msg in reversed(message):
        # Add token per 4 characters, give slight extra to make sure the limit is never reached
        tokens += len(msg["content"]) / 4 * 1.01
        # Token limit reached, stop adding messages
        if tokens > 4000:
            break
        # Add message to start of wantedMessages
        wantedMessages.insert(0, msg)
    message = wantedMessages

    # Run a chat completion
    response = openai.ChatCompletion.create(
        model="gpt-4", messages=chatQuery + message, temperature=0.7
    )
    # Log the response
    Logs.log("thread", json.dumps(chatQuery + message, indent=4), "", response)

    responseMessage = response["choices"][0]["message"]["content"]
    responseToUser = responseMessage[:]
    print("")
    print("Assistant: " + responseMessage.replace("\n", " "))
    print("")

    # Extract commands from the response, commands are within [], everything outside of [] is a response to the user
    commands = []
    hasCmdRet = False
    hasCmd = False
    while "[" in responseToUser:
        commands.append(
            responseToUser[responseToUser.find("[") + 1 : responseToUser.find("]")]
        )
        if "CMDRET" in commands[-1]:
            hasCmdRet = True
        elif "CMD" in commands[-1]:
            hasCmd = True
        responseToUser = responseToUser.replace("[" + commands[-1] + "]", "").strip()
        responseToUser = responseToUser.replace("  ", " ")

    message.append({"role": "assistant", "content": responseMessage})

    # Respond to user
    if len(responseToUser) > 0:
        print("")
        print("CineMatic: " + responseToUser.replace("\n", " "))
        print("")

    # Execute commands and return responses
    if hasCmdRet:
        returnMessage = ""
        for command in commands:
            command = command.split("~")
            if command[1] == "web_search":
                returnMessage += "[RES~" + WebSearch.advanced(command[2]) + "]"
            elif command[1] == "movie_lookup":
                # If multiple terms, split and search for each
                if len(command[2].split("¬")) > 1:
                    for term in command[2].split("¬"):
                        returnMessage += (
                            "[RES~" + Radarr.lookup_movie(term, command[3]) + "]"
                        )
                else:
                    returnMessage += (
                        "[RES~" + Radarr.lookup_movie(command[2], command[3]) + "]"
                    )
            elif command[1] == "series_lookup":
                # If multiple terms, split and search for each
                if len(command[2].split("¬")) > 1:
                    for term in command[2].split("¬"):
                        returnMessage += (
                            "[RES~" + Sonarr.lookup_series(term, command[3]) + "]"
                        )
                else:
                    returnMessage += (
                        "[RES~" + Sonarr.lookup_series(command[2], command[3]) + "]"
                    )
            elif command[1] == "memory_get":
                returnMessage += (
                    "[RES~" + Memories.get_memory(usersName, command[2]) + "]"
                )

        message.append({"role": "system", "content": returnMessage})
        print("")
        print("System: " + returnMessage.replace("\n", " "))
        print("")

        if depth < 3:
            runChatCompletion(usersName, message, relevantExamples, depth + 1)
    # Execute regular commands
    elif hasCmd:
        for command in commands:
            command = command.split("~")
            if command[1] == "movie_post":
                Radarr.add_movie(command[2], command[3])
            # elif command[1] == 'movie_delete':
            #     Radarr.delete_movie(command[2])
            elif command[1] == "movie_put":
                Radarr.put_movie(command[2])
            elif command[1] == "series_post":
                Sonarr.add_series(command[2], command[3])
            # elif command[1] == 'series_delete':
            #     Sonarr.delete_series(command[2])
            elif command[1] == "series_put":
                Sonarr.put_series(command[2])
            elif command[1] == "memory_update":
                Memories.update_memory(usersName, command[2])


# Loop prompting for input
usersName = "User"
for i in range(10):
    # Get user input
    userText = input("User: ")
    # Get timestamp seconds from epoch
    timestamp = time.time()

    # Create chat_history.json if it doesn't exist
    if not os.path.exists("chat_history.json"):
        with open("chat_history.json", "w") as f:
            json.dump({}, f)
    # Get chat history from json file
    chatHistory = {}
    with open("chat_history.json", "r") as f:
        chatHistory = json.load(f)
    # If user doesnt have a chat history, create one
    if usersName not in chatHistory:
        chatHistory[usersName] = []

    # Remove users chat history more than 20 minutes ago
    for msg in reversed(chatHistory[usersName]):
        if float(msg.split(">")[0]) < time.time() - (60 * 20):
            chatHistory[usersName].remove(msg)

    # Get relevant examples
    relevantExamples = Examples.get_examples(userText)
    # Get current message and include chatHistory
    currentMessage = []
    if len(chatHistory[usersName]) > 0:
        currentMessage.append(
            {
                "role": "user",
                "content": "Chat History: " + "|".join(chatHistory[usersName]),
            }
        )
        currentMessage.append(
            {
                "role": "assistant",
                "content": "I have noted your message history thank you",
            }
        )
    currentMessage.append({"role": "user", "content": userText})
    runChatCompletion(usersName, currentMessage, relevantExamples, 0)

    # Add userText to chatHistory with timestamp
    chatHistory[usersName].append(str(timestamp) + ">" + userText)
    # Write chatHistory into json file
    with open("chat_history.json", "w") as f:
        json.dump(chatHistory, f)
