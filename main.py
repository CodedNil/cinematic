import openai
import json
import os
import time
import discord
import random

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
LogsReview = ModuleLogs("review")

Examples = ExamplesAPI(credentials["openai"])


# Init messages
initMessages = [
    {
        "role": "user",
        "content": """You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media; always run lookups to ensure correct id, do not rely on chat history, if the data you have received does not contain what you need, you reply with the truthful answer of unknown""",
    },
    {
        "role": "user",
        "content": f"The current date is {time.strftime('%d/%m/%Y')}, the current time is {time.strftime('%H:%M:%S')}, if needing data beyond 2021 training data use a web search",
    },
]


async def runChatCompletion(
    botsMessage, usersId: str, message: list, relevantExamples: list, depth: int = 0
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
        responseToUser = (
            responseToUser.replace("[" + commands[-1] + "]", "")
            .replace("  ", " ")
            .strip()
        )

    message.append({"role": "assistant", "content": responseMessage})

    # Respond to user
    if len(responseToUser) > 0:
        # Add message into the botsMessage, emoji to show the message is in progress
        isntFinal = hasCmdRet and depth < 3
        await botsMessage.edit(content=(isntFinal and "⌛ " or "✅ ") + responseToUser)

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
                    "[RES~" + Memories.get_memory(usersId, command[2]) + "]"
                )

        message.append({"role": "system", "content": returnMessage})

        if depth < 3:
            await runChatCompletion(
                botsMessage, usersId, message, relevantExamples, depth + 1
            )
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
                Memories.update_memory(usersId, command[2])


class MyClient(discord.Client):
    """Discord bot client class"""

    async def getMessageHistory(self, message) -> list:
        """Get the message history of a message"""

        # Check if message is a reply to the bot, if it is, create a message history
        messageHistory = []
        if message.reference is not None:
            replied_to = await message.channel.fetch_message(
                message.reference.message_id
            )

            if replied_to.author.id == self.user.id:
                # If message is not completed, do nothing
                content = replied_to.content
                if not content.startswith("✅"):
                    return []
                # Remove all text after a ❗
                if "❗" in content:
                    content = content[: content.find("❗")]
                # Add the message the assistant sent
                messageHistory.append(
                    {
                        "role": "assistant",
                        "content": content.replace("\n", " ").replace("✅ ", "").strip(),
                    }
                )
                # Get the message the assistant was replying to, the users query
                if replied_to.reference is not None:
                    replied_to_to = await message.channel.fetch_message(
                        replied_to.reference.message_id
                    )
                    # Add the users query to the message history
                    messageHistory.append(
                        {
                            "role": "user",
                            "content": replied_to_to.content.replace("\n", " ")
                            .replace("<@" + str(self.user.id) + ">", "")
                            .strip(),
                        }
                    )
        # Flip the message history so it is in the correct order
        messageHistory.reverse()
        return messageHistory

    async def on_message(self, message):
        """Event handler for when a message is sent in a channel the bot has access to"""

        # Don't reply to ourselves
        if message.author.id == self.user.id:
            return

        # Check if message mentions bot
        mentionsBot = False
        if message.mentions:
            for mention in message.mentions:
                if mention.id == self.user.id:
                    mentionsBot = True
                    break
        if not mentionsBot:
            return

        # Check if message is a reply to the bot, if it is, create a message history
        messageHistory = await self.getMessageHistory(message)

        # Get users id and name
        usersId = str(message.author.id)
        usersName = message.author.name
        # Get message content, removing mentions and newlines
        userText = (
            message.content.replace("\n", " ")
            .replace("<@" + str(self.user.id) + ">", "")
            .strip()
        )

        # Reply to message
        replyMessage = [
            "Hey there! Super excited to process your message, give me just a moment... 🎬",
            "Oh, a message! Can't wait to dive into this one - I'm on it... 🎥",
            "Hey, awesome! A new message to explore! Let me work my media magic... 📺",
            "Woo-hoo! A fresh message to check out! Let me put my CineMatic touch on it... 🍿",
            "Yay, another message! Time to unleash my media passion, be right back... 📼",
            "Hey, a message! I'm so excited to process this one, just a moment... 🎞",
            "Aha! A message has arrived! Let me roll out the red carpet for it... 🎞️",
            "Ooh, a new message to dissect! Allow me to unleash my inner film buff... 🎦",
            "Lights, camera, action! Time to process your message with a cinematic twist... 📽️",
            "Hooray, a message to dig into! Let's make this a blockbuster experience... 🌟",
            "Greetings! Your message has caught my eye, let me give it the star treatment... 🎟️",
            "Popcorn's ready! Let me take a closer look at your message like a true film fanatic... 🍿",
            "Woohoo! A message to analyze! Let me work on it while humming my favorite movie tunes... 🎶",
            "A new message to dive into! Let me put on my director's hat and get to work... 🎩",
            "And... action! Time to process your message with my media expertise... 📹",
            "Hold on to your seats! I'm about to process your message with the excitement of a movie premiere... 🌆",
            "Sending your message to the cutting room! Let me work on it like a skilled film editor... 🎞️",
            "A message has entered the scene! Let me put my media prowess to work on it... 🎭",
            "Your message is the star of the show! Let me process it with the passion of a true cinephile... 🌟",
            "In the spotlight! Let me process your message with the enthusiasm of a film festival enthusiast... 🎪",
            "Curtain up! Your message takes center stage, and I'm ready to give it a standing ovation... 🎦",
        ]
        if message == None:
            return
        botsMessage = await message.reply("⌛ " + random.choice(replyMessage))

        # Get relevant examples, combine user text with message history
        userTextHistory = ""
        for message in messageHistory:
            if message["role"] == "user":
                userTextHistory += message["content"] + "\n"
        relevantExamples = Examples.get_examples(userTextHistory + userText)
        # Get current messages
        currentMessage = []
        currentMessage.append({"role": "user", "content": f"Hi my name is {usersName}"})
        currentMessage.append(
            {"role": "assistant", "content": f"Hi, how can I help you?"}
        )
        # Add message history
        for message in messageHistory:
            currentMessage.append(message)
        # Add users message
        currentMessage.append({"role": "user", "content": userText})

        await runChatCompletion(
            botsMessage, usersId, currentMessage, relevantExamples, 0
        )

    async def on_raw_reaction_add(self, payload):
        """When you thumbs down a bots message, it submits it for manual review"""

        channel = self.get_channel(payload.channel_id)
        message = await channel.fetch_message(payload.message_id)

        # If message is not from bot, do nothing
        if message.author.id != self.user.id:
            return
        # If message is not completed, or already submitted, do nothing
        if not message.content.startswith("✅") or "❗" in message.content:
            return
        # If reaction emoji is not thumbs down, do nothing
        if payload.emoji.name != "👎":
            return

        # Submit message for manual review
        LogsReview.log_simple(message.content)
        await message.edit(
            content=message.content
            + "\n❗ This message has been submitted for manual review."
        )


intents = discord.Intents.default()
intents.message_content = True

client = MyClient(intents=intents)
client.run(credentials["discord"])
