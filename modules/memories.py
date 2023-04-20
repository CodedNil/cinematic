import os
import json
import openai
from modules.module_logs import ModuleLogs


class MemoriesAPI:
    """Memories API, used to store and retrieve memories"""

    def __init__(self, openai_key: str) -> None:
        """Initialise with openai key"""
        openai.api_key = openai_key

        # Create logs
        self.logs = ModuleLogs("memories")

    def get_memory(self, user: str, query: str) -> str:
        """Get a memory from the users memory file with ai querying"""

        # Get users memories
        memories = {}
        if os.path.exists("memories.json"):
            with open("memories.json") as json_file:
                memories = json.load(json_file)

        if user in memories:
            userMemories = memories[user]

            # Search with gpt through the users memory file
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo",
                messages=[
                    {
                        "role": "user",
                        "content": "You are a memory access assistant, you view a memory file and query it for information",
                    },
                    {
                        "role": "user",
                        "content": "memories:requested all 7 abc movies, enjoyed eastworld",
                    },
                    {
                        "role": "user",
                        "content": "user requested abc movie 2?",
                    },
                    {
                        "role": "assistant",
                        "content": "yes user requested abc 2",
                    },
                    {
                        "role": "user",
                        "content": "user requested eastworld?",
                    },
                    {
                        "role": "assistant",
                        "content": "no user has not requested eastworld, but they mentioned they enjoyed it",
                    },
                    {
                        "role": "user",
                        "content": "the above are examples, do you understand?",
                    },
                    {
                        "role": "assistant",
                        "content": "yes I understand those are examples and future messages are the real ones",
                    },
                    {"role": "user", "content": "memories:" + userMemories},
                    {"role": "user", "content": query},
                ],
                temperature=0.7,
            )

            # Log the response
            self.logs.log("get_memory", userMemories, query, response)

            return response["choices"][0]["message"]["content"]
        else:
            return "no memories"

    def update_memory(self, user: str, query: str) -> None:
        """Update a memory in the users memory file with ai"""

        # Get users memories
        memories = {}
        if os.path.exists("memories.json"):
            with open("memories.json") as json_file:
                memories = json.load(json_file)

        if user in memories:
            userMemories = memories[user]
        else:
            userMemories = ""

        # Add the new memory with gpt through the users memory file
        response = openai.ChatCompletion.create(
            model="gpt-3.5-turbo",
            messages=[
                {
                    "role": "user",
                    "content": "You are a memory writer assistant, you view a memory file and update it with information, you write extremely brief summaries",
                },
                {
                    "role": "user",
                    "content": "memories:enjoyed movie puppet 1, wants series eastworld",
                },
                {"role": "user", "content": "Add 'loved movie stingate 1995'"},
                {
                    "role": "assistant",
                    "content": "enjoyed movie puppet 1 and loved movie stingate 1995, wants series eastworld",
                },
                {"role": "user", "content": "Add 'doesnt want series eastworld'"},
                {
                    "role": "assistant",
                    "content": "enjoyed movie puppet 1 and loved movie stingate 1995",
                },
                {
                    "role": "user",
                    "content": "the above are examples, do you understand?",
                },
                {
                    "role": "assistant",
                    "content": "yes I understand those are examples and future messages are the real ones",
                },
                {"role": "user", "content": "memories:" + userMemories},
                {"role": "user", "content": f"Add '{query}'"},
            ],
            temperature=0.7,
        )

        # Log the response
        self.logs.log("update_memory", userMemories, query, response)

        # Update the users memories
        memories[user] = response["choices"][0]["message"]["content"]

        # Save the memories to memories.json
        with open("memories.json", "w") as outfile:
            json.dump(memories, outfile)
