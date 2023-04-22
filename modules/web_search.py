import json
import openai
import time
import requests
import asyncio
from trafilatura import fetch_url, extract
from modules.module_logs import ModuleLogs


class WebAPI:
    """Class to handle web search and scraping"""

    def __init__(self, openai_key: str) -> None:
        """Initialise with credentials"""
        openai.api_key = openai_key

        # Create logs
        self.logs = ModuleLogs("web")

    async def basic(self, query: str = "", numResults: int = 3) -> dict:
        """Perform a DuckDuckGo Search and return the results as a dict"""
        try:
            search = requests.get(
                "https://ddg-api.herokuapp.com/search",
                params={
                    "query": query,
                    "limit": numResults,
                },
            )
            return search.json()
        except requests.exceptions.RequestException as e:
            return {}

    async def fetch_site(self, url: str) -> str:
        """Fetch a site and return a snippet of text"""
        downloaded = fetch_url(url)
        if downloaded:
            # Extract text data from website
            result = extract(downloaded)

            return result.replace("\n", " ; ")
        return ""

    async def advanced(self, query: str = "") -> str:
        """Perform a DuckDuckGo Search, then scrape the sites through gpt to return the answer to the prompt"""
        sitesToScrape = 3
        search = await self.basic(query, sitesToScrape)

        # Expand the snippets for each site for any that return the page within a few seconds
        tasks = []
        for index, result in enumerate(search):
            # Asyncio task to fetch the site and replace the snippet
            task = asyncio.create_task(self.fetch_site(result["link"]))
            tasks.append(task)

        # Wait for all the sites to be fetched, or timeout
        await asyncio.sleep(4)
        bigSnippets = 0
        for index, task in enumerate(tasks):
            if task.done() and task.result() != "":
                search[index]["snippet"] = task.result()
                bigSnippets += 1
            else:
                task.cancel()

        blob = ""
        # Based on sites to scrape, divide a quota of characters between them
        for index, result in enumerate(search):
            if len(result["snippet"]) > 500:
                result["snippet"] = result["snippet"][: int(6000 / bigSnippets)]
            blob += f'[{index}] {result["link"]}: {result["snippet"]}\n'

        date = time.strftime("%d/%m/%Y")

        # Run a chat completion to get the answer to the prompt from results
        response = openai.ChatCompletion.create(
            model="gpt-3.5-turbo",
            messages=[
                {
                    "role": "user",
                    "content": blob,
                },
                {
                    "role": "user",
                    "content": f"Current date: {date}\nYour answers should be on one line and compact\nWith the provided information, {query}",
                },
            ],
            temperature=0.7,
        )

        # Log the response
        self.logs.log("answer", json.dumps(result).replace("\n", " ")[:200], query, response)
        return response["choices"][0]["message"]["content"].strip()
