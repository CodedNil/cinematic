import json
import openai
import requests
from modules.module_logs import ModuleLogs

from duckduckgo_search import ddg
from bs4 import BeautifulSoup


class WebAPI:
    """Class to handle web search and scraping"""

    def __init__(self, openai_key: str) -> None:
        """Initialise with credentials"""
        openai.api_key = openai_key

        # Create logs
        self.logs = ModuleLogs("web")

    def basic(self, query: str = "", numResults: int = 4) -> dict:
        """Perform a DuckDuckGo Search and return the results as a JSON string"""
        search_results = []
        if not query:
            return json.dumps(search_results)

        results = ddg(query, max_results=numResults)
        if not results:
            return json.dumps(search_results)

        for j in results:
            search_results.append(j)

        return search_results

    def advanced(self, query: str = "") -> str:
        """Perform a DuckDuckGo Search, parse the results through gpt to get the top pick site based on the query, then scrape that website through gpt to return the answer to the prompt"""
        search_results = self.basic(query, 8)

        # Run a chat completion to get the top pick site
        response = openai.ChatCompletion.create(
            model="gpt-3.5-turbo",
            messages=[
                {
                    "role": "user",
                    "content": json.dumps(search_results)
                    + "\nAbove is the results of a web search that I just performed for "
                    + query
                    + ", which one seems the best to scrape in more detail? Give me the numeric value of it (0, 1, 2, 3, etc.)",
                }
            ],
            temperature=0.7,
        )
        # Log the response
        self.logs.log("website_choice", json.dumps(search_results), query, response)

        responseNumber = response["choices"][0]["message"]["content"]
        # Test if the response number str is single digit number
        if (
            not responseNumber.isdigit()
            or int(responseNumber) > len(search_results) - 1
        ):
            responseNumber = 0
        else:
            responseNumber = int(responseNumber)

        # Scrape the site, fetch only the main content
        url = search_results[responseNumber]["href"]
        responseText = ""
        try:
            response = requests.get(url, timeout=5)
            response.raise_for_status()
            soup = BeautifulSoup(response.text, "html.parser")
            responseText = soup.get_text().replace("  ", " ")
        except requests.exceptions.RequestException as e:
            # Try one more web search with a different website
            responseNumber = responseNumber + 1
            if responseNumber > len(search_results) - 1:
                responseNumber = 0

            # Scrape the site, fetch only the main content
            url = search_results[responseNumber]["href"]
            try:
                response = requests.get(url, timeout=5)
                response.raise_for_status()
                soup = BeautifulSoup(response.text, "html.parser")
                responseText = soup.get_text().replace("  ", " ")
            except requests.exceptions.RequestException as e:
                responseText = "Error " + str(e)

        # Split the main content into chunks
        lines = (line.strip() for line in responseText.splitlines())
        chunks = (phrase.strip() for line in lines for phrase in line.split(" "))
        # Group chunks to fit within the character limit
        chunkGroups = [""]
        for chunk in chunks:
            if len(chunkGroups[-1]) + len(chunk) < 12000:
                chunkGroups[-1] += chunk + " "
            else:
                chunkGroups.append(chunk + " ")
        chunkGroups = [x.strip() for x in chunkGroups]

        # Summarise or get answers to each chunk with gpt
        summary = ""
        for chunk in chunkGroups:
            # Run a chat completion to get the answer to the prompt
            response = openai.ChatCompletion.create(
                model="gpt-3.5-turbo",
                messages=[
                    {
                        "role": "user",
                        "content": f"{chunk}\nAbove is a chunk of the main content of the website {url}, give your best possible answer to '{query}'. If there is no good answer to the prompt, summarise the chunk instead",
                    }
                ],
                temperature=0.7,
            )
            # Log the response
            self.logs.log("summarise", chunk, query, response)

            summary += response["choices"][0]["message"]["content"] + " "
        summary = summary.strip()

        # Run a chat completion to get the answer to the prompt from the summarised chunks
        response = openai.ChatCompletion.create(
            model="gpt-3.5-turbo",
            messages=[
                {
                    "role": "user",
                    "content": f"{summary}\nAbove is the summarised chunks of the main content of the website {url}, give your best possible answer to '{query}'",
                },
            ],
            temperature=0.7,
        )
        # Log the response
        self.logs.log("answer", summary.replace("\n", " - "), query, response)

        return response["choices"][0]["message"]["content"]
