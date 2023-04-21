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

    def get_memory(self, usersId: str, query: str) -> str:
        """Get a memory from the users memory file with ai querying"""

        # Get users memories
        memories = {}
        if os.path.exists("memories.json"):
            with open("memories.json") as json_file:
                memories = json.load(json_file)

        if usersId in memories:
            userMemories = memories[usersId]

            # Search with gpt through the users memory file
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo",
                messages=[
                    {
                        "role": "system",
                        "content": "You are a memory access assistant, you view a memory file and query it for information",
                    },
                    {
                        "role": "system",
                        "content": "Memories:TV series wanted: N/A | Movies wanted: All 7 ABC movies | Opinions: Enjoyed series Eastworld",
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
                    {"role": "user", "content": "Memories:" + userMemories},
                    {"role": "user", "content": query},
                ],
                temperature=0.7,
            )

            # Log the response
            self.logs.log("get_memory", userMemories, query, response)

            return response["choices"][0]["message"]["content"]
        else:
            return "no memories"

    def update_memory(self, usersId: str, query: str) -> None:
        """Update a memory in the users memory file with ai"""

        # Get users memories
        memories = {}
        if os.path.exists("memories.json"):
            with open("memories.json") as json_file:
                memories = json.load(json_file)

        if usersId in memories:
            userMemories = memories[usersId]
        else:
            userMemories = ""

        # Add the new memory with gpt through the users memory file
        response = openai.ChatCompletion.create(
            model="gpt-4",
            messages=[
                {
                    "role": "system",
                    "content": "You are a memory writer assistant, you view a memory file and update it with information, you write extremely brief summaries",
                },
                {
                    "role": "system",
                    "content": "Memories:TV series wanted: Eastworld | Movies wanted: N/A | Opinions: Enjoyed movie Puppet 1",
                },
                {"role": "user", "content": "Add 'loved movie stingate 1995'"},
                {
                    "role": "assistant",
                    "content": "TV series wanted: Eastworld | Movies wanted: N/A | Opinions: Enjoyed movie Puppet 1 and loved movie Stingate 1995",
                },
                {"role": "user", "content": "Add 'doesnt want series eastworld'"},
                {
                    "role": "assistant",
                    "content": "TV series wanted: N/A | Movies wanted: N/A | Opinions: Enjoyed movie Puppet 1 and loved movie Stingate 1995",
                },
                {
                    "role": "user",
                    "content": "the above are examples, do you understand?",
                },
                {
                    "role": "assistant",
                    "content": "yes I understand those are examples and future messages are the real ones",
                },
                {"role": "user", "content": "Memories:" + userMemories},
                {"role": "user", "content": f"Add '{query}'"},
            ],
            temperature=0.7,
        )

        # Log the response
        self.logs.log("update_memory", userMemories, query, response)

        # Update the users memories
        memories[usersId] = response["choices"][0]["message"]["content"]

        # Save the memories to memories.json
        with open("memories.json", "w") as outfile:
            json.dump(memories, outfile)
