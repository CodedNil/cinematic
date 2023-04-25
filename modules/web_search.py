import json
import openai
import time
import requests
from concurrent.futures import ThreadPoolExecutor, as_completed
from trafilatura import fetch_url, extract
import modules.module_logs as ModuleLogs


class WebAPI:
    """Class to handle web search and scraping"""

    def __init__(self, openai_key: str) -> None:
        """Initialise with credentials"""
        openai.api_key = openai_key

    def basic(self, query: str = "", numResults: int = 3) -> dict:
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

    def fetch_site(self, url: str) -> str:
        """Fetch a site and return a snippet of text"""
        downloaded = fetch_url(url)
        if downloaded:
            # Extract text data from website
            result = extract(downloaded)

            return result.replace("\n", " ; ")
        return ""

    def advanced(self, query: str = "") -> str:
        """Perform a DuckDuckGo Search, then scrape the sites through gpt to return the answer to the prompt"""
        sitesToScrape = 3
        search_results = self.basic(query, sitesToScrape)

        # Expand the snippets for each site for any that return the page within a few seconds
        tasks = []
        # Create a thread to fetch the site for each search result
        with ThreadPoolExecutor() as executor:
            for result in search_results:
                task = executor.submit(self.fetch_site, result["link"])
                tasks.append(task)

        # Wait for all the threads to complete or timeout
        bigSnippets = 0
        for future in as_completed(tasks, timeout=4):
            index = tasks.index(future)
            if future.done() and future.result() != "":
                search_results[index]["snippet"] = future.result()
                bigSnippets += 1
            else:
                future.cancel()

        blob = ""
        # Based on sites to scrape, divide a quota of characters between them
        for index, result in enumerate(search_results):
            if len(result["snippet"]) > 500:
                result["snippet"] = result["snippet"][: int(6000 / bigSnippets)]
            blob += f'[{index}] {result["link"]}: {result["snippet"]}\n'

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
                    "content": f"Your answers should be on one line and compact lists have comma separations\Based on the given information, {query}",
                },
            ],
            temperature=0.7,
        )

        # Log the response
        ModuleLogs.log_ai(
            "web",
            "answer",
            json.dumps(result).replace("\n", " ")[:200],
            query,
            response,
        )
        return response["choices"][0]["message"]["content"].strip()
