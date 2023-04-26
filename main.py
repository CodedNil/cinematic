import openai
import json
import time
import discord
import random
import asyncio
import tiktoken
import threading
from concurrent.futures import ThreadPoolExecutor

# Modules
import modules.module_logs as ModuleLogs
from modules.memories import MemoriesAPI
from modules.web_search import WebAPI
from modules.series_api import SeriesAPI
from modules.movies_api import MoviesAPI
from modules.examples import ExamplesAPI

# API credentials
credentials = json.loads(open("credentials.json").read())

openai.api_key = credentials["openai"]
encoding = tiktoken.get_encoding("cl100k_base")

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

Examples = ExamplesAPI(credentials["openai"])


# Init messages
initMessages = [
    {
        "role": "user",
        "content": "You are media management assistant called CineMatic, enthusiastic, knowledgeable and passionate about all things media; always run lookups to ensure correct id, do not rely on chat history, if the data you have received does not contain what you need, you reply with the truthful answer of unknown, responses should all be on one line (with comma separation) and compact language",
    },
    {
        "role": "user",
        "content": f"The current date is {time.strftime('%d/%m/%Y')}, the current time is {time.strftime('%H:%M:%S')}, if needing data beyond 2021 training data you can use a web search",
    },
]

replyMessages = [
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


def run_command(usersName: str, usersId: str, args: list) -> None:
    """Run a command"""
    if args[1] == "movie_post":
        Radarr.add_movie(args[2], args[3])
    # elif args[1] == 'movie_delete':
    #     Radarr.delete_movie(args[2])
    elif args[1] == "movie_put":
        Radarr.put_movie(args[2])
    elif args[1] == "series_post":
        Sonarr.add_series(args[2], args[3])
    # elif args[1] == 'series_delete':
    #     Sonarr.delete_series(args[2])
    elif args[1] == "series_put":
        Sonarr.put_series(args[2])
    elif args[1] == "memory_update":
        Memories.update_memory(usersName, usersId, args[2])


def run_command_ret(usersName: str, usersId: str, args: list) -> str:
    """Run a command with a return"""
    returnMessage = ""
    if args[1] == "web_search":
        returnMessage = "[RES~" + WebSearch.advanced(args[2]) + "]"
    elif args[1] == "movie_lookup":
        # If multiple terms, split and search for each
        if len(args[2].split("¬")) > 1:
            for term in args[2].split("¬"):
                returnMessage = "[RES~" + Radarr.lookup_movie(term, args[3]) + "]"
        else:
            returnMessage = "[RES~" + Radarr.lookup_movie(args[2], args[3]) + "]"
    elif args[1] == "series_lookup":
        # If multiple terms, split and search for each
        if len(args[2].split("¬")) > 1:
            for term in args[2].split("¬"):
                returnMessage = "[RES~" + Sonarr.lookup_series(term, args[3]) + "]"
        else:
            returnMessage = "[RES~" + Sonarr.lookup_series(args[2], args[3]) + "]"
    elif args[1] == "memory_get":
        returnMessage = "[RES~" + Memories.get_memory(usersName, usersId, args[2]) + "]"
    return returnMessage


async def runChatCompletion(
    botsMessage,
    botsStartMessage: str,
    usersName: str,
    usersId: str,
    message: list,
    relevantExamples: list,
    depth: int = 0,
) -> None:
    # Get the chat query to enter
    chatQuery = initMessages.copy()
    chatQuery += relevantExamples

    # Calculate tokens of the messages, GPT-3.5-Turbo has max tokens 4,096
    tokens = 0
    # Add up tokens in chatQuery
    for msg in chatQuery:
        tokens += len(encoding.encode(msg["content"]))
    # Add up tokens in message, but only add until limit is reached then remove earliest messages
    wantedMessages = []
    for msg in reversed(message):
        # Add token per 4 characters, give slight extra to make sure the limit is never reached
        tokens += len(encoding.encode(msg["content"]))
        # Token limit reached, stop adding messages
        if tokens > 4000:
            break
        # Add message to start of wantedMessages
        wantedMessages.insert(0, msg)
    message = wantedMessages

    # Run a chat completion and stream it in
    response = openai.ChatCompletion.create(
        model="gpt-4",
        messages=chatQuery + message,
        temperature=0.7,
        stream=True,
    )
    # Collect message to user and commands
    fullMessage = ""
    lastMessage = ""
    userMessage = ""
    commandMessage = ""
    commandTasks = []
    # Last edit of the discord message
    lastEdit = time.time()
    # Is the output currently within a command?
    in_command = False
    # Iterate through the stream of events
    with ThreadPoolExecutor() as executor:
        for chunk in response:
            # Collect the chunk of text
            chunk_message = chunk["choices"][0]["delta"].get("content", "")
            # Collect non command text
            if "[" in chunk_message:
                in_command = True
                userMessage += chunk_message.split("[")[0]
                commandMessage = ""
            if in_command:
                commandMessage += chunk_message
            else:
                userMessage += chunk_message
                # Send message to user on discord
                if userMessage != lastMessage:
                    lastMessage = userMessage
                # Edit the message if it hasnt been updated in 0.5 seconds
                if time.time() - lastEdit > 0.5:
                    lastEdit = time.time()
                    await botsMessage.edit(
                        content=botsStartMessage + "⌛ " + userMessage.replace("\n", " ")
                    )
            if "]" in chunk_message:
                in_command = False
                # Run commands
                commandArgs = commandMessage.strip()[1:-1].split("~")
                if commandArgs[0] == "CMDRET":
                    # Create a thread to run the command and append it to the list of tasks
                    commandTasks.append(
                        executor.submit(
                            run_command_ret, usersName, usersId, commandArgs
                        )
                    )
                elif commandArgs[0] == "CMD":
                    # Create a thread to run the command
                    executor.submit(run_command, usersName, usersId, commandArgs)
            # Collect the full message
            fullMessage += chunk_message

    # Wait for all threads to finish
    commandReplies = []
    for task in commandTasks:
        commandReplies.append(task.result())
    # TODO: Log the response
    # ModuleLogs.log_ai("main", "thread", json.dumps(message, indent=4), "", response)

    # Respond to user with full message
    if len(userMessage) > 0:
        # Add message into the botsMessage, emoji to show the message is in progress
        isntFinal = len(commandReplies) and depth < 3
        await botsMessage.edit(
            content=botsStartMessage
            + (isntFinal and "⌛ " or "✅ ")
            + userMessage.replace("\n", " ")
        )

    # Do another loop with these new replies
    if len(commandReplies) > 0:
        message.append({"role": "assistant", "content": fullMessage})
        message.append({"role": "system", "content": "".join(commandReplies)})
        if depth < 3:
            await runChatCompletion(
                botsMessage,
                botsStartMessage,
                usersName,
                usersId,
                message,
                relevantExamples,
                depth + 1,
            )


async def processChat(
    botsMessage,
    botsStartMessage: str,
    replyMessage: str,
    usersName: str,
    usersId: str,
    messageHistory: list,
    userText: str,
) -> None:
    """Process the discord message asynchronously"""
    # Get relevant examples, combine user text with message history
    userTextHistory = ""
    for message in messageHistory:
        if message["role"] == "user":
            userTextHistory += message["content"] + "\n"

    # Don't reply to non media queries
    messages = [
        {
            "role": "system",
            "content": "You determine if a users message is irrelevant to you, is it related to movies, series, asking for recommendations, changing resolution, adding or removing media, checking disk space, viewing users memories etc? You reply with a single word answer, yes or no.",
        },
        {
            "role": "user",
            "content": f"{userTextHistory + userText}\nDo not respond to the above message, is the above text irrelevant? Reply with a single word answer, only say yes if certain",
        },
    ]
    response = openai.ChatCompletion.create(
        model="gpt-4", messages=messages, temperature=0.7, max_tokens=2, n=3
    )
    ModuleLogs.log_ai("relevance", "check", userTextHistory + userText, "", response)
    # If the ai responsed with yes in all of its choices, say I am a media bot and return
    isValid = False
    for choice in response["choices"]:
        if not choice["message"]["content"].lower().startswith("yes"):
            isValid = True
    if not isValid:
        await botsMessage.edit(
            content=f"{botsStartMessage}❌ Hi, I'm a media bot. I can help you with media related questions. What would you like to know or achieve?"
        )
        return
    # Edit the message for stage 2
    await botsMessage.edit(content=f"{botsStartMessage}⌛ 2/3 {replyMessage}")

    relevantExamples = Examples.get_examples(userTextHistory + userText)
    # Edit the message for stage 3
    await botsMessage.edit(content=f"{botsStartMessage}⌛ 3/3 {replyMessage}")

    # Get current messages
    currentMessage = []
    currentMessage.append({"role": "user", "content": f"Hi my name is {usersName}"})
    currentMessage.append({"role": "assistant", "content": f"Hi, how can I help you?"})
    # Add message history
    for message in messageHistory:
        currentMessage.append(message)
    # Add users message
    currentMessage.append({"role": "user", "content": userText})

    # Run chat completion
    await runChatCompletion(
        botsMessage,
        botsStartMessage,
        usersName,
        usersId,
        currentMessage,
        relevantExamples,
        0,
    )


class MyClient(discord.Client):
    """Discord bot client class"""

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

        # If message starts with ! then it is a debug message, ignore it
        if message.content.startswith("!"):
            return

        # Check if message is a reply to the bot, if it is, create a message history
        messageHistory = []
        if message.reference is not None:
            replied_to = await message.channel.fetch_message(
                message.reference.message_id
            )
            if replied_to.author.id == self.user.id:
                # See if the message is completed
                if "✅" not in replied_to.content:
                    return
                # Split message by lines
                content = replied_to.content.split("\n")
                for msg in content:
                    # If the line is a reply to the bot, add it to the message history
                    if msg.startswith("✅"):
                        messageHistory.append(
                            {
                                "role": "assistant",
                                "content": msg.replace("✅ ", "☑️ ").strip(),
                            }
                        )
                    elif msg.startswith("☑️"):
                        messageHistory.append(
                            {
                                "role": "assistant",
                                "content": msg.strip(),
                            }
                        )
                    # If the line is a reply to the user, add it to the message history
                    elif msg.startswith("💬"):
                        messageHistory.append(
                            {
                                "role": "user",
                                "content": msg.strip(),
                            }
                        )

        # Get users id and name
        usersId = str(message.author.id)
        usersName = message.author.name
        print("Message from " + usersName + " (" + usersId + "): " + message.content)
        ModuleLogs.log(
            "messages",
            "Message from " + usersName + " (" + usersId + "): " + message.content,
        )
        # Get message content, removing mentions and newlines
        userText = (
            message.content.replace("\n", " ")
            .replace("<@" + str(self.user.id) + ">", "")
            .strip()
        )

        # Reply to message
        if message == None:
            return
        botsStartMessage = ""
        for msg in messageHistory:
            botsStartMessage += msg["content"] + "\n"
        botsStartMessage += f"💬 {userText}\n"
        replyMessage = random.choice(replyMessages)
        botsMessage = await message.reply(f"{botsStartMessage}⌛ 1/3 {replyMessage}")

        # Process message in a thread
        asyncio.create_task(
            processChat(
                botsMessage,
                botsStartMessage,
                replyMessage,
                usersName,
                usersId,
                messageHistory,
                userText,
            )
        )

    async def on_raw_reaction_add(self, payload):
        """When you thumbs down a bots message, it submits it for manual review"""

        channel = self.get_channel(payload.channel_id)
        message = await channel.fetch_message(payload.message_id)

        # If message is not from bot, do nothing
        if message.author.id != self.user.id:
            return
        # If message is not completed, or already submitted, do nothing
        if (
            not ("✅" in message.content or "❌" in message.content)
            or "❗" in message.content
        ):
            return
        # If reaction emoji is not thumbs down, do nothing
        if payload.emoji.name != "👎":
            return

        # Submit message for manual review
        ModuleLogs.log("review", message.content)
        await message.edit(
            content=message.content
            + "\n❗ This message has been submitted for manual review."
        )


intents = discord.Intents.default()
intents.message_content = True

client = MyClient(intents=intents)
client.run(credentials["discord"])
